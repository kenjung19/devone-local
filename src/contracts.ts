export type RuntimeKind = "php" | "mysql" | "caddy";
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
  resolved: Record<string, string>;
  status: string;
  https: string;
  discovered_at: number;
  updated_at: number;
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
}
export interface PhpConfig {
  directives: Record<string, string>;
  extensions: string[];
}
export type Action =
  | {
      type:
        | "scan"
        | "start"
        | "stop"
        | "restart"
        | "dns"
        | "trust"
        | "refresh_catalog";
    }
  | { type: "import"; manifest: Manifest; source: string }
  | { type: "install" | "remove" | "default" | "validate"; runtime: RuntimeRef }
  | {
      type: "override";
      site_id: string;
      kind: "php" | "mysql";
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
