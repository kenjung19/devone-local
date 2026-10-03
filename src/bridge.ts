import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Action, Snapshot, Response } from "./contracts";
export const desktop = isTauri();
export const bridge = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  execute: (action: Action) => invoke<Response>("execute", { action }),
  logFiles: () => invoke<string[]>("log_files"),
  readLog: (name: string) => invoke<string>("read_log", { name }),
};
