import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Error shape returned by every Rust command. */
export interface ApiError {
  code:
    | "vault_missing"
    | "vault_exists"
    | "locked"
    | "wrong_password"
    | "invalid_recovery_key"
    | "weak_password"
    | "too_many_attempts"
    | "corrupt_vault"
    | "invalid"
    | "crypto"
    | "db"
    | "io"
    | "json";
  message: string;
  retry_after_secs?: number;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return "Something went wrong.";
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface AppSettings {
  autoLockMinutes: number;
}

export const api = {
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  createVault: (password: string) =>
    invoke<{ recoveryKey: string }>("vault_create", { password }),
  unlock: (password: string) => invoke<void>("vault_unlock", { password }),
  recover: (recoveryKey: string, newPassword: string) =>
    invoke<{ recoveryKey: string }>("vault_recover", { recoveryKey, newPassword }),
  lock: () => invoke<void>("vault_lock"),
  activity: () => invoke<void>("activity"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  setAutoLock: (minutes: number) => invoke<AppSettings>("set_auto_lock", { minutes }),
  changePassword: (currentPassword: string, newPassword: string) =>
    invoke<void>("change_password", { currentPassword, newPassword }),
};

export function onVaultLocked(cb: (reason: string) => void): Promise<UnlistenFn> {
  return listen<string>("vault-locked", (e) => cb(e.payload));
}
