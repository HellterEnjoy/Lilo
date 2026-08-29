CREATE TABLE IF NOT EXISTS launches (
    installation_id TEXT NOT NULL,
    launch_date TEXT NOT NULL,
    app_version TEXT NOT NULL,
    received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (installation_id, launch_date)
);

CREATE TABLE IF NOT EXISTS feature_usage (
    installation_id TEXT NOT NULL,
    usage_date TEXT NOT NULL,
    app_version TEXT NOT NULL,
    feature_name TEXT NOT NULL,
    usage_count INTEGER NOT NULL CHECK (
        usage_count >= 1 AND usage_count <= 1000
    ),
    received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        installation_id,
        usage_date,
        app_version,
        feature_name
    )
);

CREATE INDEX IF NOT EXISTS idx_launches_date
ON launches (launch_date);

CREATE INDEX IF NOT EXISTS idx_feature_usage_date
ON feature_usage (usage_date);

CREATE INDEX IF NOT EXISTS idx_feature_usage_feature
ON feature_usage (feature_name, usage_date);

CREATE TABLE IF NOT EXISTS github_traffic_daily (
    metric_date TEXT PRIMARY KEY,
    views_count INTEGER NOT NULL,
    views_unique INTEGER NOT NULL,
    clones_count INTEGER NOT NULL,
    clones_unique INTEGER NOT NULL,
    collected_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS github_traffic_referrers (
    snapshot_date TEXT NOT NULL,
    referrer TEXT NOT NULL,
    view_count INTEGER NOT NULL,
    unique_visitors INTEGER NOT NULL,
    PRIMARY KEY (snapshot_date, referrer)
);

CREATE TABLE IF NOT EXISTS github_traffic_paths (
    snapshot_date TEXT NOT NULL,
    path TEXT NOT NULL,
    title TEXT NOT NULL,
    view_count INTEGER NOT NULL,
    unique_visitors INTEGER NOT NULL,
    PRIMARY KEY (snapshot_date, path)
);
