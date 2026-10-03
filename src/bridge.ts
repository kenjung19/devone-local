import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  Action,
  Snapshot,
  Response,
  InstallProgress,
  Manifest,
  RuntimeKind,
} from "./contracts";
export const desktop = isTauri();
export const bridge = {
  inspectImport: (kind: RuntimeKind, source: string) =>
    invoke<Manifest>("inspect_import", { kind, source }),
  progress: () => invoke<InstallProgress | null>("install_progress"),
  snapshot: () => invoke<Snapshot>("snapshot"),
  execute: (action: Action) => invoke<Response>("execute", { action }),
  logFiles: () => invoke<string[]>("log_files"),
  readLog: (name: string) => invoke<string>("read_log", { name }),
};
