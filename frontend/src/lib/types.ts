/**
 * 与 Rust 后端模型保持一致的前端类型定义。
 * 后端 commands 返回 / 接收的结构均映射到本文件中。
 */

export interface GutFilePair {
  table_name: string;
  date: string;
  time: string;
  sequence: string;
  flg_path: string;
  dat_path: string;
  estimated_rows?: number;
}

export type DbType =
  | "mysql"
  | "postgres"
  | "opengauss"
  | "txsql"
  | "tdsql"
  | "gaussdb"
  | "oracle"
  | "dameng";

export interface DatabaseConfig {
  db_type: DbType;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  schema?: string;
}

export interface LlmConfig {
  api_url: string;
  api_key: string;
  model: string;
}

export type Severity = "info" | "warning" | "error";

export interface PreCheckResult {
  check_name: string;
  passed: boolean;
  message: string;
  severity: Severity;
}

export type LoadStatus =
  | "pending"
  | "loading"
  | "completed"
  | "completed_with_errors"
  | "failed";

export interface LoadProgress {
  table_name: string;
  total_rows: number;
  loaded_rows: number;
  failed_rows: number;
  status: LoadStatus;
  speed: number;
  elapsed_ms: number;
}

export interface TableReport {
  table_name: string;
  row_count: number;
  success_count: number;
  failed_count: number;
  elapsed_ms: number;
  speed: number;
  errors: string[];
}

export interface LoadReport {
  total_tables: number;
  total_rows: number;
  success_rows: number;
  failed_rows: number;
  success_rate: number;
  total_elapsed_ms: number;
  avg_speed: number;
  table_reports: TableReport[];
}

export const DB_TYPE_LABEL: Record<DbType, string> = {
  mysql: "MySQL",
  postgres: "PostgreSQL",
  opengauss: "openGauss",
  txsql: "TXSQL",
  tdsql: "TDSQL",
  gaussdb: "GaussDB",
  oracle: "Oracle",
  dameng: "达梦 DM",
};

const getBrowserPlatform = () => {
  const nav = globalThis.navigator;
  const userAgentPlatform =
    (nav as Navigator & { userAgentData?: { platform?: string } }).userAgentData
      ?.platform ?? "";
  return `${userAgentPlatform} ${nav?.platform ?? ""} ${nav?.userAgent ?? ""}`;
};

export const DM_SUPPORTED_ON_CURRENT_PLATFORM = (() => {
  const platform = getBrowserPlatform().toLowerCase();
  return platform.includes("win") || platform.includes("linux");
})();

export const DB_TYPE_OPTIONS = Object.entries(DB_TYPE_LABEL).filter(([key]) => {
  return key !== "dameng" || DM_SUPPORTED_ON_CURRENT_PLATFORM;
}) as [DbType, string][];

export const normalizeDbTypeForCurrentPlatform = (dbType: DbType): DbType => {
  if (dbType === "dameng" && !DM_SUPPORTED_ON_CURRENT_PLATFORM) {
    return "mysql";
  }
  return dbType;
};

export const DB_TYPE_PLATFORM_NOTE: Partial<Record<DbType, string>> = {
  dameng: "仅 Windows x64、Linux x64 与 Linux arm64 可用",
};

export const DM_UNSUPPORTED_MESSAGE =
  "达梦 DM 仅在 Windows x64、Linux x64 与 Linux arm64 版本可用。";

export const DB_TYPE_DEFAULT_PORT: Record<DbType, number> = {
  mysql: 3306,
  txsql: 3306,
  tdsql: 3306,
  postgres: 5432,
  opengauss: 5432,
  gaussdb: 5432,
  oracle: 1521,
  dameng: 5236,
};

export interface SavedDbConfig extends DatabaseConfig {
  id: string;
  name: string;
}
