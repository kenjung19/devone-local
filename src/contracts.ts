export type RuntimeKind = "php" | "mysql" | "caddy" | "node";
export interface RuntimeRef {
  kind: RuntimeKind;
  version: string;
}
export interface Manifest {
  runtime: string;
  version: string;
  platform: string;
  binaries: Record<string, string>;
  download?: string | null;
  sha256?: string | null;
  metadata: Record<string, string>;
}
export interface Installation {
  id: string;
  manifest: Manifest;
  relative_path: string;
  installed_at: number;
}
export interface Site {
  id: string;
  name: string;
  hostname: string;
  project_path: string;
  project_type: string;
  document_root: string;
  present: boolean;
  issue: string | null;
  overrides: Record<string, string>;
  local_overrides?: Record<string, string>;
  resolved: Record<string, string>;
  runtime_sources?: Record<string, string>;
  status: string;
  https: string;
  discovered_at: number;
  updated_at: number;
  metadata?: ProjectMetadata;
  processes?: ProjectProcess[];
}
export interface Service {
  key: string;
  pid: number | null;
  status: string;
  port: number | null;
  healthy: boolean;
  log: string;
}
export interface Database {
  runtime_id: string;
  data_path: string;
  initialized: boolean;
  port: number | null;
}
export interface Snapshot {
  developer?: Developer;
  tool_defaults?: Record<string, string>;
  dependency_tasks?: DependencyTask[];
  tools?: Tool[];
  available_tools?: Tool[];
  home: string;
  platform: string;
  sites: Site[];
  installed: Installation[];
  available: Manifest[];
  defaults: Record<string, string>;
  services: Service[];
  databases: Database[];
  issues: string[];
  dns_ready: boolean;
  ca_present: boolean;
  active: boolean;
  startup: { supported: boolean; enabled: boolean; conflict: boolean };
  environment_autostart: boolean;
  setup: {
    completed: boolean;
    home_ready: boolean;
    dns_policy: boolean;
    dns_server: boolean;
    dns_system: boolean;
    ca_present: boolean;
    ca_trusted: boolean;
    ca_upgrade_pending?: boolean;
    caddy: boolean;
    php: boolean;
    mysql: boolean;
  };
  php_settings: Record<
    string,
    { config: PhpConfig; available_extensions: string[]; original_ini: boolean }
  >;
  binary_roles: Record<string, Record<string, string>>;
  project_databases: {
    site_id: string;
    runtime_id: string;
    database_name: string;
    username: string;
    credential_ref: string;
    status: string;
  }[];
}
export interface PhpConfig {
  directives: Record<string, string>;
  extensions: string[];
}
export interface InstallProgress {
  runtime: string;
  phase: string;
  bytes: number;
  total: number | null;
  error: string | null;
}
export type Action =
  | { type: "create_project"; request: CreateRequest }
  | { type: "cancel_creation"; task_id: string }
  | { type: "save_editor"; editor: Editor }
  | {
      type: "editor";
      site_id: string | null;
      editor_id: string | null;
      operation: "open" | "default";
    }
  | {
      type: "site_preference";
      site_id: string;
      operation: "favorite" | "unfavorite" | "opened";
    }
  | { type: "mail"; operation: "start" | "stop" | "open" }
  | { type: "diagnostics"; export: boolean }
  | { type: "backup_preferences"; keep_last: number }
  | {
      type: "db_admin";
      runtime_id: string;
      operation:
        | "list"
        | "create"
        | "delete"
        | "backup"
        | "restore"
        | "connection"
        | "credential";
      database_name: string;
      username: string | null;
      confirmation: string | null;
      path: string | null;
    }
  | {
      type: "site_action";
      site_id: string;
      operation: "start" | "stop" | "restart";
    }
  | {
      type: "site_process";
      site_id: string;
      process_id: string;
      operation:
        | "start"
        | "stop"
        | "restart"
        | "remove"
        | "enable_autostart"
        | "disable_autostart"
        | "disable";
    }
  | {
      type: "save_process";
      site_id: string;
      definition: ProcessDefinition;
      replace?: boolean;
    }
  | {
      type: "tool_action";
      id: string;
      version: string;
      operation: "default" | "validate" | "remove";
      runtime_version: string | null;
    }
  | { type: "cancel_dependencies"; site_id: string }
  | { type: "save_portable"; site_id: string }
  | {
      type: "install_dependencies";
      site_id: string;
      manager: "pnpm" | "composer";
    }
  | { type: "install_tool"; id: string; version: string; node: string | null }
  | { type: "startup" | "environment_autostart"; enabled: boolean }
  | { type: "recreate_ca"; confirmed: boolean }
  | { type: "finish_setup"; skip: boolean }
  | { type: "remote_catalog"; url: string; sha256: string }
  | {
      type: "database";
      runtime: RuntimeRef;
      operation: "initialize" | "start" | "stop" | "restart" | "validate";
    }
  | { type: "provision"; site_id: string; database_name: string }
  | { type: "reveal_credential"; site_id: string }
  | {
      type:
        | "scan"
        | "start"
        | "stop"
        | "restart"
        | "dns"
        | "trust"
        | "remove_trust"
        | "refresh_catalog"
        | "remove_dns"
        | "hosts_fallback"
        | "prepare_ca"
        | "upgrade_ca"
        | "reopen_setup";
    }
  | { type: "import"; manifest: Manifest; source: string }
  | { type: "install" | "remove" | "default" | "validate"; runtime: RuntimeRef }
  | {
      type: "override";
      site_id: string;
      kind: "php" | "mysql" | "node";
      version: string | null;
    }
  | { type: "open_site" | "terminal"; site_id: string }
  | { type: "open_folder"; site_id: string | null }
  | { type: "php_config"; version: string; config: PhpConfig }
  | { type: "open_ini"; version: string };
export interface Response {
  snapshot: Snapshot;
  message: string | null;
}

export interface Tool {
  id: string;
  version: string;
  url: string;
  sha256: string;
  entry: string;
}
export interface ProcessDefinition {
  id: string;
  name: string;
  runtime: string;
  executable: string;
  args: string[];
  cwd: string;
  env: Record<string, string>;
  port: boolean;
  health?: "process_alive" | "tcp_listener" | "http" | null;
  autostart: boolean;
}
export interface ProjectProcess {
  site_id: string;
  definition: ProcessDefinition;
  enabled: boolean;
  key: string;
  assigned_port?: number | null;
  health_strategy?: string;
  status?: string;
}
export interface ProjectMetadata {
  framework: string;
  requirements: string[];
  runtimes: Record<string, string>;
  package_manager: string | null;
  package_manager_version: string | null;
  dev_script: string | null;
  scripts?: string[];
  build_script: string | null;
  route: "php_fastcgi" | "node_proxy" | "static";
  node_dependencies: boolean;
  node_dependency_state?: string;
  composer_dependency_state?: string;
  composer_dependencies: boolean;
  composer_manifest?: boolean;
  error: string | null;
}

export interface DependencyTask {
  site_id: string;
  manager: string;
  status: string;
  log: string;
  error: string | null;
}

export interface Template {
  id: string;
  name: string;
  category: string;
  strategy: string;
  version: string;
  runtimes: Record<string, string>;
  tools: string[];
  custom: boolean;
}
export interface CreateRequest {
  name: string;
  template: string;
  runtimes: Record<string, string>;
  tools: Record<string, string>;
  install_dependencies: boolean;
  database_name: string | null;
  configure_mail: boolean;
}
export interface CreationTask {
  id: string;
  template: string;
  project_name: string;
  status: string;
  stage: string;
  log: string;
  error: string | null;
  site_id: string | null;
  destination: string | null;
  created_at: number;
}
export interface Editor {
  id: string;
  name: string;
  executable: string;
  args: string[];
  category: "editor" | "database_client";
}
export interface ManagedDatabase {
  runtime_id: string;
  database_name: string;
  username: string;
  credential_ref: string;
  site_id: string | null;
  status: string;
}
export interface Backup {
  id: string;
  type: string;
  database: string;
  runtime: string;
  path: string;
  size: number;
  sha256: string;
  created_at: number;
  status: string;
}
export interface Developer {
  templates: Template[];
  template_error: string | null;
  creation_tasks: CreationTask[];
  editors: Editor[];
  default_editor: string | null;
  preferences: Record<
    string,
    { favorite: boolean; opened_at: number; created_at: number }
  >;
  managed_databases: ManagedDatabase[];
  backups: Backup[];
  mail: {
    installed: boolean;
    running: boolean;
    smtp_port: number | null;
    web_port: number | null;
  };
  database_catalog: Record<
    string,
    { name: string; system: boolean; managed: boolean }[]
  >;
  backup_preferences: { automatic: boolean; keep_last: number };
  diagnostic_report: string | null;
}
