PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS shared_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  protocol TEXT NOT NULL DEFAULT 'tcp',
  version TEXT,
  allow_relay INTEGER NOT NULL DEFAULT 0,
  network_name TEXT,
  network_secret TEXT,
  description TEXT,
  max_connections INTEGER NOT NULL DEFAULT 100,
  current_connections INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 0,
  is_approved INTEGER NOT NULL DEFAULT 0,
  qq_number TEXT,
  wechat TEXT,
  mail TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_shared_nodes_host_port_protocol
  ON shared_nodes (host, port, protocol);

CREATE TABLE IF NOT EXISTS health_records (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id INTEGER NOT NULL,
  status TEXT NOT NULL,
  response_time INTEGER,
  error_message TEXT,
  checked_at TEXT NOT NULL,
  FOREIGN KEY (node_id) REFERENCES shared_nodes(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_health_records_node_id
  ON health_records (node_id);

CREATE INDEX IF NOT EXISTS idx_health_records_checked_at
  ON health_records (checked_at);

CREATE INDEX IF NOT EXISTS idx_health_records_node_time
  ON health_records (node_id, checked_at);

CREATE INDEX IF NOT EXISTS idx_health_records_status
  ON health_records (status);

CREATE TABLE IF NOT EXISTS node_tags (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id INTEGER NOT NULL,
  tag TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY (node_id) REFERENCES shared_nodes(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_tags_node
  ON node_tags (node_id);

CREATE INDEX IF NOT EXISTS idx_node_tags_tag
  ON node_tags (tag);

CREATE UNIQUE INDEX IF NOT EXISTS uniq_node_tag_per_node
  ON node_tags (node_id, tag);
