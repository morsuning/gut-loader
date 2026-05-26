import { useEffect, useMemo, useRef } from "react";
import {
  CheckCircle2,
  CircleDashed,
  Loader2,
  Play,
  Square,
  XCircle,
  Activity,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useAppStore } from "@/stores/appStore";
import { useTauriCommands } from "@/hooks/useTauriCommands";
import type { LoadProgress as LoadProgressItem, LoadStatus } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * 步骤 4：实时加载进度展示。
 * 总进度 + 每张表的独立进度 + 实时日志 + 操作按钮 + 概览卡片。
 */
export function LoadingProgress() {
  const isLoading = useAppStore((s) => s.isLoading);
  const setIsLoading = useAppStore((s) => s.setIsLoading);
  const loadingProgress = useAppStore((s) => s.loadingProgress);
  const loadingLogs = useAppStore((s) => s.loadingLogs);
  const appendLoadingLog = useAppStore((s) => s.appendLoadingLog);
  const clearLoadingLogs = useAppStore((s) => s.clearLoadingLogs);
  const selectedDirectory = useAppStore((s) => s.selectedDirectory);
  const dbConfig = useAppStore((s) => s.dbConfig);
  const filePairs = useAppStore((s) => s.filePairs);

  const { startLoading, stopLoading } = useTauriCommands();
  const logScrollRef = useRef<HTMLDivElement>(null);

  const stats = useMemo(() => {
    const total = loadingProgress.reduce((s, p) => s + p.total_rows, 0);
    const loaded = loadingProgress.reduce((s, p) => s + p.loaded_rows, 0);
    const failed = loadingProgress.reduce((s, p) => s + p.failed_rows, 0);
    const speed = loadingProgress
      .filter((p) => p.status === "loading")
      .reduce((s, p) => s + p.speed, 0);
    const overall = total > 0 ? Math.round((loaded / total) * 100) : 0;
    return { total, loaded, failed, speed, overall };
  }, [loadingProgress]);

  // 日志滚动到底
  useEffect(() => {
    const el = logScrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [loadingLogs]);

  const handleStart = async () => {
    if (!selectedDirectory) {
      toast.warning("请先选择数据目录");
      return;
    }
    if (filePairs.length === 0) {
      toast.warning("当前没有可入库的文件对");
      return;
    }
    setIsLoading(true);
    clearLoadingLogs();
    appendLoadingLog(
      `[${formatNow()}] >> 启动加载任务  dir=${selectedDirectory}  target=${dbConfig.db_type}://${dbConfig.host}:${dbConfig.port}/${dbConfig.database}`,
    );
    appendLoadingLog(
      `[${formatNow()}] >> 文件对总数 = ${filePairs.length}`,
    );
    try {
      await startLoading(selectedDirectory, dbConfig);
    } catch (e) {
      console.error(e);
      appendLoadingLog(`[${formatNow()}] !! 启动失败：${String(e)}`);
      setIsLoading(false);
    }
  };

  const handleStop = async () => {
    appendLoadingLog(`[${formatNow()}] !! 用户请求停止任务…`);
    await stopLoading();
    setIsLoading(false);
  };

  return (
    <div className="space-y-8">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div className="space-y-2">
          <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
            STEP / 04 — Load execution
          </p>
          <h2 className="text-2xl font-semibold tracking-tight">
            执行批量入库
          </h2>
          <p className="text-sm text-muted-foreground max-w-2xl">
            实时显示每张表的加载进度、速率与失败计数；点击下方按钮开始或终止任务。
          </p>
        </div>
        <div className="flex gap-2">
          {!isLoading ? (
            <Button onClick={handleStart} className="gap-2">
              <Play className="h-4 w-4" />
              开始加载
            </Button>
          ) : (
            <Button
              onClick={handleStop}
              variant="destructive"
              className="gap-2"
            >
              <Square className="h-4 w-4" />
              终止任务
            </Button>
          )}
        </div>
      </header>

      {/* 概览卡片 */}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard
          label="总记录数"
          value={stats.total.toLocaleString()}
          unit="rows"
        />
        <StatCard
          label="已加载"
          value={stats.loaded.toLocaleString()}
          unit="rows"
          accent="emerald"
        />
        <StatCard
          label="失败"
          value={stats.failed.toLocaleString()}
          unit="rows"
          accent={stats.failed > 0 ? "destructive" : undefined}
        />
        <StatCard
          label="当前速率"
          value={Math.round(stats.speed).toLocaleString()}
          unit="rows/s"
          accent="amber"
          live={isLoading}
        />
      </div>

      {/* 总体进度 */}
      <div className="rounded-xl border bg-card p-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Activity
              className={cn(
                "h-4 w-4",
                isLoading
                  ? "animate-pulse text-amber-500"
                  : "text-muted-foreground",
              )}
            />
            <p className="text-sm font-semibold">总体进度</p>
          </div>
          <span className="font-mono text-2xl font-semibold tabular-nums">
            {stats.overall.toString().padStart(2, "0")}
            <span className="ml-0.5 text-sm text-muted-foreground">%</span>
          </span>
        </div>
        <Progress value={stats.overall} className="mt-3 h-2" />
      </div>

      {/* 表级进度列表 */}
      <section>
        <div className="flex items-baseline justify-between">
          <h3 className="text-sm font-semibold tracking-wide">每表进度</h3>
          <span className="font-mono text-xs text-muted-foreground">
            {loadingProgress.length.toString().padStart(2, "0")} tables
          </span>
        </div>

        {loadingProgress.length === 0 ? (
          <div className="mt-4 grid place-items-center rounded-xl border border-dashed bg-muted/20 px-6 py-12 text-center">
            <CircleDashed className="mb-3 h-7 w-7 text-muted-foreground/60" />
            <p className="text-sm text-muted-foreground">
              {isLoading ? "等待后端推送进度…" : "尚未开始加载"}
            </p>
          </div>
        ) : (
          <ul className="mt-4 space-y-2">
            {loadingProgress.map((p) => (
              <TableRow key={p.table_name} progress={p} />
            ))}
          </ul>
        )}
      </section>

      {/* 实时日志 */}
      <section>
        <div className="flex items-baseline justify-between">
          <h3 className="text-sm font-semibold tracking-wide">实时日志</h3>
          <span className="font-mono text-xs text-muted-foreground">
            tail -f
          </span>
        </div>
        <div className="mt-3 overflow-hidden rounded-xl border bg-zinc-950 text-zinc-100">
          <div className="flex items-center gap-1.5 border-b border-zinc-800 px-3 py-2">
            <span className="h-2 w-2 rounded-full bg-red-500/80" />
            <span className="h-2 w-2 rounded-full bg-yellow-500/80" />
            <span className="h-2 w-2 rounded-full bg-emerald-500/80" />
            <span className="ml-2 font-mono text-[11px] uppercase tracking-wider text-zinc-500">
              gut-loader/runtime.log
            </span>
          </div>
          <div
            ref={logScrollRef}
            className="h-[240px] overflow-y-auto"
          >
            <div className="px-4 py-3 font-mono text-xs leading-relaxed">
              {loadingLogs.length === 0 ? (
                <span className="text-zinc-500">// no output yet</span>
              ) : (
                loadingLogs.map((line, i) => (
                  <div key={i} className="whitespace-pre-wrap">
                    <span className="select-none text-zinc-600">
                      {(i + 1).toString().padStart(4, "0")}{" "}
                    </span>
                    <span
                      className={cn(
                        line.includes("!!")
                          ? "text-red-400"
                          : line.includes(">>")
                            ? "text-emerald-400"
                            : "text-zinc-200",
                      )}
                    >
                      {line}
                    </span>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

function TableRow({ progress }: { progress: LoadProgressItem }) {
  const pct =
    progress.total_rows > 0
      ? Math.round((progress.loaded_rows / progress.total_rows) * 100)
      : 0;
  const meta = statusVisual(progress.status);
  const Icon = meta.icon;
  return (
    <li className="rounded-lg border bg-card px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <div
            className={cn(
              "grid h-8 w-8 place-items-center rounded-md",
              meta.bg,
              meta.fg,
            )}
          >
            <Icon
              className={cn(
                "h-4 w-4",
                progress.status === "loading" && "animate-spin",
              )}
            />
          </div>
          <div>
            <p className="font-mono text-sm font-semibold">
              {progress.table_name}
            </p>
            <p className="font-mono text-xs text-muted-foreground">
              {progress.loaded_rows.toLocaleString()} /{" "}
              {progress.total_rows.toLocaleString()}
              {progress.failed_rows > 0 && (
                <span className="ml-2 text-destructive">
                  ✕ {progress.failed_rows}
                </span>
              )}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-6 font-mono text-xs">
          <Stat label="速率" value={`${Math.round(progress.speed)} r/s`} />
          <Stat label="耗时" value={formatMs(progress.elapsed_ms)} />
          <span className="w-12 text-right text-base font-semibold tabular-nums">
            {pct}%
          </span>
        </div>
      </div>
      <Progress value={pct} className="mt-3 h-1.5" />
    </li>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span className="hidden sm:inline-flex flex-col">
      <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span className="font-semibold tabular-nums text-foreground">
        {value}
      </span>
    </span>
  );
}

function StatCard({
  label,
  value,
  unit,
  accent,
  live,
}: {
  label: string;
  value: string;
  unit?: string;
  accent?: "emerald" | "destructive" | "amber";
  live?: boolean;
}) {
  const accentClass =
    accent === "emerald"
      ? "text-emerald-600 dark:text-emerald-400"
      : accent === "destructive"
        ? "text-destructive"
        : accent === "amber"
          ? "text-amber-600 dark:text-amber-400"
          : "text-foreground";

  return (
    <div className="relative overflow-hidden rounded-xl border bg-card p-5">
      <div className="flex items-center justify-between">
        <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-muted-foreground">
          {label}
        </p>
        {live && (
          <span className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-amber-600 dark:text-amber-400">
            <span className="relative inline-flex h-1.5 w-1.5">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
              <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-amber-500" />
            </span>
            LIVE
          </span>
        )}
      </div>
      <p className={cn("mt-3 font-mono text-3xl font-semibold tabular-nums", accentClass)}>
        {value}
      </p>
      {unit && (
        <p className="mt-1 font-mono text-[11px] uppercase tracking-wider text-muted-foreground">
          {unit}
        </p>
      )}
    </div>
  );
}

function statusVisual(s: LoadStatus) {
  switch (s) {
    case "completed":
      return {
        icon: CheckCircle2,
        bg: "bg-emerald-500/10",
        fg: "text-emerald-600 dark:text-emerald-400",
      };
    case "failed":
      return {
        icon: XCircle,
        bg: "bg-destructive/10",
        fg: "text-destructive",
      };
    case "loading":
      return {
        icon: Loader2,
        bg: "bg-amber-500/10",
        fg: "text-amber-600 dark:text-amber-400",
      };
    default:
      return {
        icon: CircleDashed,
        bg: "bg-muted",
        fg: "text-muted-foreground",
      };
  }
}

function formatMs(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  return `${m}m${rs.toString().padStart(2, "0")}s`;
}

function formatNow() {
  const d = new Date();
  return d.toTimeString().slice(0, 8);
}
