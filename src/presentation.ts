import type { Site } from "./contracts";
export function bindingLabel(
  site: Pick<Site, "overrides" | "resolved">,
  kind: string,
): string {
  const version = site.resolved[kind];
  if (!version) return "Not configured";
  return version + (site.overrides[kind] ? " · override" : " · default");
}
export function availableSiteCount(sites: Pick<Site, "present">[]): number {
  return sites.filter((s) => s.present).length;
}
export function runtimeBinaries(kind: string): Record<string, string> {
  if (kind === "node") return { cli: "node.exe" };
  if (kind === "php") return { cli: "php.exe", fastcgi: "php-cgi.exe" };
  if (kind === "mysql")
    return { server: "bin/mysqld.exe", admin: "bin/mysqladmin.exe" };
  return { server: "caddy.exe" };
}
