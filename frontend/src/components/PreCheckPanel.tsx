import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  RefreshCw,
  ShieldCheck,
  ShieldAlert,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useAppStore } from "@/stores/appStore";
import { useTauriCommands } from "@/hooks/useTauriCommands";
import type { PreCheckResult, Severity } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * 步骤 3：前置检查面板。汇总磁盘 / 文件 / 表结构 / 连通性等检查。
 */
export function PreCheckPanel() {
  const dbConfig = useAppStore((s) => s.dbConfig);
  const selectedDirectory = useAppStore((s) => s.selectedDirectory);
  const preCheckResults = useAppStore((s) => s.preCheckResults);
  const setPreCheckResults = useAppStore((s) => s.setPreCheckResults);
  const { runPreChecks } = useTauriCommands();

  const [running, setRunning] = useState(false);
  const hasAutoRun = useRef(false);

  const summary = useMemo(() => {
    const total = preCheckResults.length;
    const failed = preCheckResults.filter((r) => !r.passed).length;
    const errors = preCheckResults.filter(
      (r) => !r.passed && r.severity === "error",
    ).length;
    const warnings = preCheckResults.filter(
      (r) => r.severity === "warning" && !r.passed,
    ).length;
    return { total, failed, errors, warnings, passed: total - failed };
  }, [preCheckResults]);

  const blocking = summary.errors > 0;
  const allGreen =
    summary.total > 0 && summary.failed === 0 && summary.warnings === 0;

  const handleRun = async () => {
    if (!selectedDirectory) {
      toast.warning("请先在步骤 1 选择数据目录");
      return;
    }
    setRunning(true);
    try {
      const results = await runPreChecks(selectedDirectory, dbConfig);
      setPreCheckResults(results);
      if (results.length === 0) {
        toast.info("后端尚未返回检查项", {
          description: "Mock 模式：前端流程已就绪",
        });
      }
    } finally {
      setRunning(false);
    }
  };

  // 组件挂载时自动触发前置检查
  useEffect(() => {
    if (selectedDirectory && !hasAutoRun.current) {
      hasAutoRun.current = true;
      handleRun();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="space-y-8">
      <header className="space-y-2">
        <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
          STEP / 03 — Pre-flight checks
        </p>
        <h2 className="text-2xl font-semibold tracking-tight">入库前置校验</h2>
        <p className="text-sm text-muted-foreground max-w-2xl">
          检查磁盘空间、文件完整性、表结构一致性、目标库连通性等。出现红色错误项时，
          流程将被阻塞，必须修复后才能进入加载阶段。
        </p>
      </header>

      <div className="rounded-xl border bg-card p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div
              className={cn(
                "grid h-11 w-11 place-items-center rounded-lg",
                blocking
                  ? "bg-destructive/10 text-destructive"
                  : allGreen
                    ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                    : "bg-primary/5 text-primary",
              )}
            >
              {blocking ? (
                <ShieldAlert className="h-5 w-5" />
              ) : (
                <ShieldCheck className="h-5 w-5" />
              )}
            </div>
            <div>
              <p className="text-xs font-mono uppercase tracking-wider text-muted-foreground">
                Pre-flight status
              </p>
              <p className="text-sm font-semibold">
                {summary.total === 0
                  ? "尚未运行检查"
                  : blocking
                    ? `检测到 ${summary.errors} 项阻断性错误`
                    : allGreen
                      ? "全部检查项通过"
                      : `通过 ${summary.passed} / ${summary.total}，含 ${summary.warnings} 项警告`}
              </p>
            </div>
          </div>

          <Button onClick={handleRun} disabled={running} className="gap-2">
            {running ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            {running ? "检查进行中…" : "重新检查"}
          </Button>
        </div>
      </div>

      <section>
        <div className="flex items-baseline justify-between">
          <h3 className="text-sm font-semibold tracking-wide">检查项</h3>
          <span className="font-mono text-xs text-muted-foreground">
            {summary.total.toString().padStart(2, "0")} checks
          </span>
        </div>

        {running && summary.total === 0 ? (
          <div className="mt-4 grid place-items-center rounded-xl border border-dashed bg-muted/20 px-6 py-16">
            <Loader2 className="mb-3 h-7 w-7 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">正在执行预检…</p>
          </div>
        ) : preCheckResults.length === 0 ? (
          <div className="mt-4 grid place-items-center rounded-xl border border-dashed bg-muted/20 px-6 py-16 text-center">
            <ShieldCheck className="mb-3 h-7 w-7 text-muted-foreground/60" />
            <p className="text-sm text-muted-foreground">
              点击「重新检查」以执行前置校验
            </p>
          </div>
        ) : (
          <ul className="mt-4 space-y-2">
            {preCheckResults.map((r, idx) => (
              <CheckItem key={`${r.check_name}-${idx}`} result={r} index={idx} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function CheckItem({
  result,
  index,
}: {
  result: PreCheckResult;
  index: number;
}) {
  const sev = severityVisual(result);
  const Icon = sev.icon;
  return (
    <li
      className={cn(
        "flex items-start gap-4 rounded-lg border bg-card px-4 py-3 transition-colors",
        sev.borderClass,
      )}
    >
      <div
        className={cn(
          "grid h-9 w-9 shrink-0 place-items-center rounded-md",
          sev.bg,
          sev.fg,
        )}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs text-muted-foreground">
            {(index + 1).toString().padStart(2, "0")}
          </span>
          <span className="font-medium">{result.check_name}</span>
          <Badge variant="outline" className={cn("text-xs", sev.badgeClass)}>
            {sev.label}
          </Badge>
        </div>
        <p className="mt-1 break-words text-sm text-muted-foreground">
          {result.message}
        </p>
      </div>
    </li>
  );
}

function severityVisual(r: PreCheckResult) {
  if (r.passed) {
    return {
      icon: CheckCircle2,
      label: "通过",
      bg: "bg-emerald-500/10",
      fg: "text-emerald-600 dark:text-emerald-400",
      borderClass: "border-l-2 border-l-emerald-500/60",
      badgeClass:
        "border-emerald-500/40 text-emerald-700 dark:text-emerald-400",
    };
  }
  const sev: Severity = r.severity;
  if (sev === "error") {
    return {
      icon: XCircle,
      label: "失败",
      bg: "bg-destructive/10",
      fg: "text-destructive",
      borderClass: "border-l-2 border-l-destructive",
      badgeClass: "border-destructive/40 text-destructive",
    };
  }
  if (sev === "warning") {
    return {
      icon: AlertTriangle,
      label: "警告",
      bg: "bg-amber-500/10",
      fg: "text-amber-600 dark:text-amber-400",
      borderClass: "border-l-2 border-l-amber-500/60",
      badgeClass: "border-amber-500/40 text-amber-700 dark:text-amber-400",
    };
  }
  return {
    icon: AlertTriangle,
    label: "信息",
    bg: "bg-muted",
    fg: "text-muted-foreground",
    borderClass: "border-l-2 border-l-border",
    badgeClass: "",
  };
}
