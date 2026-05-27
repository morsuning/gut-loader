import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";

import { useAppStore } from "@/stores/appStore";
import type {
  LoadProgress,
  LoadReport,
  TableReport,
} from "@/lib/types";

/**
 * 订阅后端在加载过程中通过 Tauri 事件系统推送的 4 类事件，
 * 并把 payload 同步到 zustand store / sonner toast。
 *
 * 事件契约：
 * - `loading-progress` -> `LoadProgress`
 * - `table-completed` -> `TableReport`
 * - `loading-completed` -> `LoadReport`
 * - `loading-error` -> `string`
 *
 * 仅在应用根组件中调用一次即可，订阅会随组件卸载自动解绑。
 */
export function useLoadingEvents() {
  const setLoadingProgress = useAppStore((s) => s.setLoadingProgress);
  const appendLoadingLog = useAppStore((s) => s.appendLoadingLog);
  const setIsLoading = useAppStore((s) => s.setIsLoading);
  const setReport = useAppStore((s) => s.setReport);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const wire = async () => {
      const u1 = await listen<LoadProgress>("loading-progress", (event) => {
        const p = event.payload;
        const list = useAppStore.getState().loadingProgress;
        const idx = list.findIndex((x) => x.table_name === p.table_name);
        const next = idx >= 0 ? [...list] : [...list, p];
        if (idx >= 0) next[idx] = p;
        setLoadingProgress(next);
      });
      unlisteners.push(u1);

      const u2 = await listen<TableReport>("table-completed", (event) => {
        const t = event.payload;
        // 同步更新进度数组：将已完成表的状态标记为 completed，确保总进度计算正确
        const list = useAppStore.getState().loadingProgress;
        const idx = list.findIndex((x) => x.table_name === t.table_name);
        if (idx >= 0) {
          const next = [...list];
          next[idx] = {
            ...next[idx],
            loaded_rows: t.success_count,
            failed_rows: t.failed_count,
            status: t.failed_count > 0 ? "completed_with_errors" : "completed",
            speed: t.speed,
            elapsed_ms: t.elapsed_ms,
          };
          setLoadingProgress(next);
        }
        appendLoadingLog(
          `[${formatNow()}] >> 表 ${t.table_name} 完成: 成功 ${t.success_count} / 失败 ${t.failed_count} / ${formatMs(t.elapsed_ms)}`,
        );
      });
      unlisteners.push(u2);

      const u3 = await listen<LoadReport>("loading-completed", (event) => {
        const r = event.payload;
        setReport(r);
        setIsLoading(false);
        appendLoadingLog(
          `[${formatNow()}] >> 全部完成: ${r.total_tables} 表 / ${r.success_rows.toLocaleString()} 行 / ${formatMs(r.total_elapsed_ms)}`,
        );
        toast.success("加载完成", {
          description: `${r.total_tables} 表 · 成功率 ${(r.success_rate * 100).toFixed(1)}%`,
        });
      });
      unlisteners.push(u3);

      const u4 = await listen<string>("loading-error", (event) => {
        const msg = event.payload;
        appendLoadingLog(`[${formatNow()}] !! 错误: ${msg}`);
        toast.error("加载错误", { description: msg });
      });
      unlisteners.push(u4);
    };

    void wire();

    return () => {
      for (const u of unlisteners) {
        try {
          u();
        } catch {
          // ignore
        }
      }
    };
  }, [setLoadingProgress, appendLoadingLog, setIsLoading, setReport]);
}

function formatNow() {
  const d = new Date();
  return d.toTimeString().slice(0, 8);
}

function formatMs(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  return `${m}m${rs.toString().padStart(2, "0")}s`;
}
