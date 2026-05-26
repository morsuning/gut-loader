import { create } from "zustand";
import type {
  DatabaseConfig,
  GutFilePair,
  LlmConfig,
  LoadProgress,
  LoadReport,
  PreCheckResult,
  SavedDbConfig,
} from "@/lib/types";
import { normalizeDbTypeForCurrentPlatform } from "@/lib/types";

const initialDbConfig: DatabaseConfig = {
  db_type: "mysql",
  host: "127.0.0.1",
  port: 3306,
  database: "",
  username: "",
  password: "",
  schema: "",
};

const LLM_STORAGE_KEY = "gut-loader-llm-config";
const DB_CONFIGS_STORAGE_KEY = "gut-loader-saved-db-configs";

const loadSavedDbConfigs = (): SavedDbConfig[] => {
  try {
    const saved = localStorage.getItem(DB_CONFIGS_STORAGE_KEY);
    if (saved) {
      return JSON.parse(saved);
    }
  } catch {
    // ignore parse errors
  }
  return [];
};

const loadSavedLlmConfig = (): LlmConfig => {
  try {
    const saved = localStorage.getItem(LLM_STORAGE_KEY);
    if (saved) {
      return JSON.parse(saved);
    }
  } catch {
    // ignore parse errors
  }
  return { api_url: "", api_key: "", model: "" };
};

const initialLlmConfig: LlmConfig = loadSavedLlmConfig();

interface AppState {
  /** 当前向导步骤索引：0 文件选择 / 1 数据库 / 2 前置检查 / 3 加载 / 4 报告 */
  currentStep: number;
  setStep: (step: number) => void;

  selectedDirectory: string;
  filePairs: GutFilePair[];
  setSelectedDirectory: (dir: string) => void;
  setFilePairs: (pairs: GutFilePair[]) => void;

  dbConfig: DatabaseConfig;
  setDbConfig: (config: Partial<DatabaseConfig>) => void;

  llmConfig: LlmConfig;
  setLlmConfig: (config: Partial<LlmConfig>) => void;

  preCheckResults: PreCheckResult[];
  setPreCheckResults: (results: PreCheckResult[]) => void;

  loadingProgress: LoadProgress[];
  isLoading: boolean;
  loadingLogs: string[];
  setLoadingProgress: (progress: LoadProgress[]) => void;
  setIsLoading: (loading: boolean) => void;
  appendLoadingLog: (line: string) => void;
  clearLoadingLogs: () => void;

  report: LoadReport | null;
  setReport: (report: LoadReport) => void;

  savedDbConfigs: SavedDbConfig[];
  saveDbConfig: (name: string) => void;
  deleteSavedDbConfig: (id: string) => void;
  loadSavedDbConfig: (id: string) => void;

  reset: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  currentStep: 0,
  setStep: (step) => set({ currentStep: step }),

  selectedDirectory: "",
  filePairs: [],
  setSelectedDirectory: (dir) => set({ selectedDirectory: dir }),
  setFilePairs: (pairs) => set({ filePairs: pairs }),

  dbConfig: initialDbConfig,
  setDbConfig: (config) =>
    set((state) => ({
      dbConfig: {
        ...state.dbConfig,
        ...config,
        db_type: normalizeDbTypeForCurrentPlatform(
          (config.db_type ?? state.dbConfig.db_type) as DatabaseConfig["db_type"],
        ),
      },
    })),

  llmConfig: initialLlmConfig,
  setLlmConfig: (config) =>
    set((state) => {
      const newConfig = { ...state.llmConfig, ...config };
      try {
        localStorage.setItem(LLM_STORAGE_KEY, JSON.stringify(newConfig));
      } catch {
        // ignore storage errors
      }
      return { llmConfig: newConfig };
    }),

  preCheckResults: [],
  setPreCheckResults: (results) => set({ preCheckResults: results }),

  loadingProgress: [],
  isLoading: false,
  loadingLogs: [],
  setLoadingProgress: (progress) => set({ loadingProgress: progress }),
  setIsLoading: (loading) => set({ isLoading: loading }),
  appendLoadingLog: (line) =>
    set((state) => ({ loadingLogs: [...state.loadingLogs, line].slice(-500) })),
  clearLoadingLogs: () => set({ loadingLogs: [] }),

  report: null,
  setReport: (report) => set({ report }),

  savedDbConfigs: loadSavedDbConfigs(),
  saveDbConfig: (name) =>
    set((state) => {
      const entry: SavedDbConfig = {
        ...state.dbConfig,
        password: "",
        id: Date.now().toString(36),
        name,
      };
      const next = [...state.savedDbConfigs, entry];
      try {
        localStorage.setItem(DB_CONFIGS_STORAGE_KEY, JSON.stringify(next));
      } catch {
        // ignore storage errors
      }
      return { savedDbConfigs: next };
    }),
  deleteSavedDbConfig: (id) =>
    set((state) => {
      const next = state.savedDbConfigs.filter((c) => c.id !== id);
      try {
        localStorage.setItem(DB_CONFIGS_STORAGE_KEY, JSON.stringify(next));
      } catch {
        // ignore storage errors
      }
      return { savedDbConfigs: next };
    }),
  loadSavedDbConfig: (id) =>
    set((state) => {
      const found = state.savedDbConfigs.find((c) => c.id === id);
      if (!found) return {};
      const { id: _id, name: _name, ...rest } = found;
      return {
        dbConfig: {
          ...rest,
          password: "",
          db_type: normalizeDbTypeForCurrentPlatform(rest.db_type),
        },
      };
    }),

  reset: () =>
    set({
      currentStep: 0,
      selectedDirectory: "",
      filePairs: [],
      dbConfig: initialDbConfig,
      preCheckResults: [],
      loadingProgress: [],
      isLoading: false,
      loadingLogs: [],
      report: null,
    }),
}));
