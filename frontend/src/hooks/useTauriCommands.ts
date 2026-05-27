import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionTestResult,
  DatabaseConfig,
  GutFilePair,
  LlmConfig,
  LoadReport,
  PreCheckResult,
} from "@/lib/types";

/**
 * 封装与 Rust 后端的所有 IPC 调用。
 * 后端命令尚未实现时，invoke 会抛错，这里统一捕获并返回兜底数据，避免 UI 崩溃。
 */
export function useTauriCommands() {
  const scanDirectory = async (path: string): Promise<GutFilePair[]> => {
    try {
      return await invoke<GutFilePair[]>("scan_directory", { path });
    } catch (e) {
      console.error("scan_directory failed:", e);
      return [];
    }
  };

  const runPreChecks = async (
    path: string,
    dbConfig: DatabaseConfig,
  ): Promise<PreCheckResult[]> => {
    try {
      return await invoke<PreCheckResult[]>("run_pre_checks", {
        path,
        dbConfig,
      });
    } catch (e) {
      console.error("run_pre_checks failed:", e);
      return [];
    }
  };

  const testConnection = async (config: DatabaseConfig): Promise<ConnectionTestResult> => {
    try {
      const ok = await invoke<boolean>("test_connection", { config });
      return { ok };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("test_connection failed:", msg);
      return { ok: false, error: msg };
    }
  };

  const getAppLogs = async (): Promise<string[]> => {
    try {
      return await invoke<string[]>("get_app_logs");
    } catch (e) {
      console.error("get_app_logs failed:", e);
      return [];
    }
  };

  const clearAppLogs = async (): Promise<void> => {
    try {
      await invoke("clear_app_logs");
    } catch (e) {
      console.error("clear_app_logs failed:", e);
    }
  };

  const startLoading = async (
    path: string,
    dbConfig: DatabaseConfig,
  ): Promise<void> => {
    try {
      await invoke("start_loading", { path, dbConfig });
    } catch (e) {
      console.error("start_loading failed:", e);
    }
  };

  const stopLoading = async (): Promise<void> => {
    try {
      await invoke("stop_loading");
    } catch (e) {
      console.error("stop_loading failed:", e);
    }
  };

  const parseDbInfo = async (
    text: string,
    llmConfig: LlmConfig,
  ): Promise<Partial<DatabaseConfig>> => {
    try {
      return await invoke<Partial<DatabaseConfig>>("parse_db_info", {
        text,
        llmConfig,
      });
    } catch (e) {
      console.error("parse_db_info failed:", e);
      return {};
    }
  };

  const testLlmConnection = async (config: LlmConfig): Promise<boolean> => {
    try {
      return await invoke<boolean>("test_llm_connection", { config });
    } catch (e) {
      console.error("test_llm_connection failed:", e);
      return false;
    }
  };

  const getReport = async (): Promise<LoadReport | null> => {
    try {
      return await invoke<LoadReport | null>("get_report");
    } catch (e) {
      console.error("get_report failed:", e);
      return null;
    }
  };

  const saveReport = async (path: string): Promise<string> => {
    return await invoke<string>("save_report", { path });
  };

  return {
    scanDirectory,
    runPreChecks,
    testConnection,
    startLoading,
    stopLoading,
    parseDbInfo,
    testLlmConnection,
    getReport,
    saveReport,
    getAppLogs,
    clearAppLogs,
  };
}
