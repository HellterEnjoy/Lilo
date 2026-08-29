const repository = process.env.GITHUB_REPOSITORY;
const githubToken = process.env.GH_TRAFFIC_TOKEN;
const workerUrl = process.env.LILO_ANALYTICS_WORKER_URL;
const ingestToken = process.env.LILO_TRAFFIC_INGEST_TOKEN;

if (!repository || !githubToken || !workerUrl || !ingestToken) {
  throw new Error(
    "GITHUB_REPOSITORY, GH_TRAFFIC_TOKEN, LILO_ANALYTICS_WORKER_URL and LILO_TRAFFIC_INGEST_TOKEN are required",
  );
}

const githubHeaders = {
  Accept: "application/vnd.github+json",
  Authorization: `Bearer ${githubToken}`,
  "X-GitHub-Api-Version": "2026-03-10",
  "User-Agent": "Lilo traffic collector",
};

async function github(path) {
  const response = await fetch(`https://api.github.com/repos/${repository}${path}`, {
    headers: githubHeaders,
  });
  if (!response.ok) {
    throw new Error(`GitHub ${path} failed with HTTP ${response.status}`);
  }
  return response.json();
}

const [viewsResponse, clonesResponse, referrers, paths] = await Promise.all([
  github("/traffic/views?per=day"),
  github("/traffic/clones?per=day"),
  github("/traffic/popular/referrers"),
  github("/traffic/popular/paths"),
]);

const payload = {
  collected_date: new Date().toISOString().slice(0, 10),
  views: viewsResponse.views,
  clones: clonesResponse.clones,
  referrers,
  paths,
};

const response = await fetch(
  `${workerUrl.replace(/\/$/, "")}/v1/github-traffic`,
  {
    method: "POST",
    headers: {
      Authorization: `Bearer ${ingestToken}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  },
);

if (!response.ok) {
  throw new Error(
    `Worker ingestion failed with HTTP ${response.status}: ${await response.text()}`,
  );
}

const result = await response.json();
console.log(`Stored ${result.daily_rows} daily GitHub traffic rows.`);
