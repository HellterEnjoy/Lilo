const ALLOWED_FEATURES = new Set([
  "note_created",
  "daily_note_opened",
  "quick_capture_saved",
  "template_note_created",
  "template_inserted",
  "markdown_formatting_used",
  "search_used",
  "saved_search_created",
  "command_palette_used",
  "wiki_link_opened",
  "graph_opened",
  "tag_filter_used",
  "attachment_added",
  "note_pinned",
  "folder_created",
  "trash_restored",
  "backup_restored",
  "markdown_imported",
  "vault_exported",
  "zen_mode_enabled",
  "always_on_top_enabled",
]);

const UUID_V4 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function json(data, status = 200) {
  return Response.json(data, {
    status,
    headers: {
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    },
  });
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function validDate(value, maximumPastDays = 1) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    return false;
  }

  const timestamp = Date.parse(`${value}T00:00:00Z`);
  if (Number.isNaN(timestamp)) {
    return false;
  }
  if (new Date(timestamp).toISOString().slice(0, 10) !== value) {
    return false;
  }

  const today = new Date().toISOString().slice(0, 10);
  const todayTimestamp = Date.parse(`${today}T00:00:00Z`);
  const difference = todayTimestamp - timestamp;
  const oneDay = 24 * 60 * 60 * 1000;
  return difference >= -oneDay && difference <= maximumPastDays * oneDay;
}

function validVersion(value) {
  return (
    typeof value === "string" &&
    /^[0-9A-Za-z][0-9A-Za-z.+_-]{0,31}$/.test(value)
  );
}

function validateInstallationId(value) {
  return typeof value === "string" && UUID_V4.test(value);
}

async function readJson(request, maximumBytes = 8192) {
  const contentType = request.headers.get("Content-Type") ?? "";
  if (!contentType.toLowerCase().includes("application/json")) {
    throw new Error("Content-Type must be application/json");
  }

  const text = await request.text();
  if (new TextEncoder().encode(text).length > maximumBytes) {
    throw new Error("Request is too large");
  }
  return JSON.parse(text);
}

function validateFeatures(features) {
  if (!isObject(features)) {
    return { valid: false, error: "features must be an object" };
  }

  const entries = Object.entries(features);
  if (entries.length > ALLOWED_FEATURES.size) {
    return { valid: false, error: "Too many features" };
  }

  for (const [featureName, usageCount] of entries) {
    if (!ALLOWED_FEATURES.has(featureName)) {
      return { valid: false, error: `Unknown feature: ${featureName}` };
    }
    if (!Number.isInteger(usageCount) || usageCount < 1 || usageCount > 1000) {
      return { valid: false, error: `Invalid count for: ${featureName}` };
    }
  }

  return { valid: true, entries };
}

function constantTimeEqual(left, right) {
  const leftBytes = new TextEncoder().encode(left);
  const rightBytes = new TextEncoder().encode(right);
  let difference = leftBytes.length ^ rightBytes.length;
  const length = Math.max(leftBytes.length, rightBytes.length);

  for (let index = 0; index < length; index += 1) {
    difference |=
      (leftBytes[index] ?? 0) ^ (rightBytes[index] ?? 0);
  }
  return difference === 0;
}

function validTrafficMetric(metric) {
  return (
    isObject(metric) &&
    Number.isInteger(metric.count) &&
    metric.count >= 0 &&
    Number.isInteger(metric.uniques) &&
    metric.uniques >= 0
  );
}

async function acceptDailyReport(request, env) {
  let data;
  try {
    data = await readJson(request);
  } catch {
    return json({ error: "Invalid JSON request" }, 400);
  }

  if (!isObject(data)) {
    return json({ error: "Request must be an object" }, 400);
  }
  if (!validateInstallationId(data.installation_id)) {
    return json({ error: "Invalid installation_id" }, 400);
  }
  if (!validDate(data.date)) {
    return json({ error: "Invalid date" }, 400);
  }
  if (!validVersion(data.app_version)) {
    return json({ error: "Invalid app_version" }, 400);
  }

  const featureValidation = validateFeatures(data.features);
  if (!featureValidation.valid) {
    return json({ error: featureValidation.error }, 400);
  }

  const statements = [
    env.DB.prepare(
      `INSERT OR IGNORE INTO launches
       (installation_id, launch_date, app_version)
       VALUES (?, ?, ?)`,
    ).bind(data.installation_id, data.date, data.app_version),
  ];

  for (const [featureName, usageCount] of featureValidation.entries) {
    statements.push(
      env.DB.prepare(
        `INSERT INTO feature_usage
         (installation_id, usage_date, app_version, feature_name, usage_count)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (installation_id, usage_date, app_version, feature_name)
         DO UPDATE SET
           usage_count = MAX(feature_usage.usage_count, excluded.usage_count),
           received_at = CURRENT_TIMESTAMP`,
      ).bind(
        data.installation_id,
        data.date,
        data.app_version,
        featureName,
        usageCount,
      ),
    );
  }

  // Individual installation rows are retained for at most 90 days.
  statements.push(
    env.DB.prepare(
      "DELETE FROM feature_usage WHERE usage_date < date('now', '-90 days')",
    ),
    env.DB.prepare(
      "DELETE FROM launches WHERE launch_date < date('now', '-90 days')",
    ),
  );

  try {
    await env.DB.batch(statements);
  } catch {
    return json({ error: "Database temporarily unavailable" }, 503);
  }

  return json({
    accepted: true,
    accepted_features: featureValidation.entries.length,
  });
}

async function deleteInstallation(request, env) {
  let data;
  try {
    data = await readJson(request);
  } catch {
    return json({ error: "Invalid JSON request" }, 400);
  }

  if (!isObject(data) || !validateInstallationId(data.installation_id)) {
    return json({ error: "Invalid installation_id" }, 400);
  }

  try {
    await env.DB.batch([
      env.DB.prepare(
        "DELETE FROM feature_usage WHERE installation_id = ?",
      ).bind(data.installation_id),
      env.DB.prepare(
        "DELETE FROM launches WHERE installation_id = ?",
      ).bind(data.installation_id),
    ]);
  } catch {
    return json({ error: "Database temporarily unavailable" }, 503);
  }

  return json({ deleted: true });
}

async function acceptGithubTraffic(request, env) {
  const expectedToken = env.GITHUB_TRAFFIC_INGEST_TOKEN;
  const authorization = request.headers.get("Authorization") ?? "";
  const suppliedToken = authorization.startsWith("Bearer ")
    ? authorization.slice("Bearer ".length)
    : "";

  if (
    typeof expectedToken !== "string" ||
    expectedToken.length < 32 ||
    !constantTimeEqual(suppliedToken, expectedToken)
  ) {
    return json({ error: "Unauthorized" }, 401);
  }

  let data;
  try {
    data = await readJson(request, 64 * 1024);
  } catch {
    return json({ error: "Invalid JSON request" }, 400);
  }

  if (
    !isObject(data) ||
    !validDate(data.collected_date) ||
    !Array.isArray(data.views) ||
    !Array.isArray(data.clones) ||
    !Array.isArray(data.referrers) ||
    !Array.isArray(data.paths) ||
    data.views.length > 14 ||
    data.clones.length > 14 ||
    data.referrers.length > 10 ||
    data.paths.length > 10
  ) {
    return json({ error: "Invalid traffic payload" }, 400);
  }

  const daily = new Map();
  for (const item of data.views) {
    const date = String(item.timestamp ?? "").slice(0, 10);
    if (!validDate(date, 14) || !validTrafficMetric(item)) {
      return json({ error: "Invalid views metric" }, 400);
    }
    daily.set(date, {
      views_count: item.count,
      views_unique: item.uniques,
      clones_count: 0,
      clones_unique: 0,
    });
  }
  for (const item of data.clones) {
    const date = String(item.timestamp ?? "").slice(0, 10);
    if (!validDate(date, 14) || !validTrafficMetric(item)) {
      return json({ error: "Invalid clones metric" }, 400);
    }
    const metric = daily.get(date) ?? {
      views_count: 0,
      views_unique: 0,
      clones_count: 0,
      clones_unique: 0,
    };
    metric.clones_count = item.count;
    metric.clones_unique = item.uniques;
    daily.set(date, metric);
  }

  const statements = [];
  for (const [date, metric] of daily) {
    statements.push(
      env.DB.prepare(
        `INSERT INTO github_traffic_daily
         (metric_date, views_count, views_unique, clones_count, clones_unique)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (metric_date) DO UPDATE SET
           views_count = excluded.views_count,
           views_unique = excluded.views_unique,
           clones_count = excluded.clones_count,
           clones_unique = excluded.clones_unique,
           collected_at = CURRENT_TIMESTAMP`,
      ).bind(
        date,
        metric.views_count,
        metric.views_unique,
        metric.clones_count,
        metric.clones_unique,
      ),
    );
  }

  statements.push(
    env.DB.prepare(
      "DELETE FROM github_traffic_referrers WHERE snapshot_date = ?",
    ).bind(data.collected_date),
    env.DB.prepare(
      "DELETE FROM github_traffic_paths WHERE snapshot_date = ?",
    ).bind(data.collected_date),
  );

  for (const item of data.referrers) {
    if (
      !isObject(item) ||
      typeof item.referrer !== "string" ||
      item.referrer.length > 255 ||
      !validTrafficMetric(item)
    ) {
      return json({ error: "Invalid referrer metric" }, 400);
    }
    statements.push(
      env.DB.prepare(
        `INSERT INTO github_traffic_referrers
         (snapshot_date, referrer, view_count, unique_visitors)
         VALUES (?, ?, ?, ?)`,
      ).bind(data.collected_date, item.referrer, item.count, item.uniques),
    );
  }

  for (const item of data.paths) {
    if (
      !isObject(item) ||
      typeof item.path !== "string" ||
      item.path.length > 1024 ||
      typeof item.title !== "string" ||
      item.title.length > 1024 ||
      !validTrafficMetric(item)
    ) {
      return json({ error: "Invalid path metric" }, 400);
    }
    statements.push(
      env.DB.prepare(
        `INSERT INTO github_traffic_paths
         (snapshot_date, path, title, view_count, unique_visitors)
         VALUES (?, ?, ?, ?, ?)`,
      ).bind(
        data.collected_date,
        item.path,
        item.title,
        item.count,
        item.uniques,
      ),
    );
  }

  try {
    await env.DB.batch(statements);
  } catch {
    return json({ error: "Database temporarily unavailable" }, 503);
  }

  return json({ accepted: true, daily_rows: daily.size });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === "GET" && url.pathname === "/health") {
      return json({ status: "ok", service: "lilo-analytics" });
    }
    if (request.method === "POST" && url.pathname === "/v1/daily") {
      return acceptDailyReport(request, env);
    }
    if (request.method === "DELETE" && url.pathname === "/v1/data") {
      return deleteInstallation(request, env);
    }
    if (
      request.method === "POST" &&
      url.pathname === "/v1/github-traffic"
    ) {
      return acceptGithubTraffic(request, env);
    }

    return json({ error: "Not found" }, 404);
  },
};
