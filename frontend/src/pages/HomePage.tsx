import { lazy, Suspense, useCallback, useMemo } from "react";
import appPackage from "../../package.json";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronRight,
  Database,
  FileSearch,
  PlayCircle,
  RefreshCcw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { useAppStore } from "@/stores/appStore";
import { useLoadingEvents } from "@/hooks/useLoadingEvents";
import { cn } from "@/lib/utils";

import { FileSelector } from "@/components/FileSelector";
import { DatabaseConfig } from "@/components/DatabaseConfig";
import { PreCheckPanel } from "@/components/PreCheckPanel";
import { LoadingProgress } from "@/components/LoadingProgress";

const ReportView = lazy(() =>
  import("@/components/ReportView").then((m) => ({ default: m.ReportView })),
);

interface StepDef {
  index: number;
  code: string;
  title: string;
  hint: string;
  icon: React.ComponentType<{ className?: string }>;
  render: () => JSX.Element;
}

const STEPS: StepDef[] = [
  {
    index: 0,
    code: "01",
    title: "源文件",
    hint: "扫描 GUT 文件对",
    icon: FileSearch,
    render: () => <FileSelector />,
  },
  {
    index: 1,
    code: "02",
    title: "目标库",
    hint: "配置入库连接",
    icon: Database,
    render: () => <DatabaseConfig />,
  },
  {
    index: 2,
    code: "03",
    title: "前置检查",
    hint: "校验环境与结构",
    icon: ShieldCheck,
    render: () => <PreCheckPanel />,
  },
  {
    index: 3,
    code: "04",
    title: "执行加载",
    hint: "实时进度与日志",
    icon: PlayCircle,
    render: () => <LoadingProgress />,
  },
  {
    index: 4,
    code: "05",
    title: "查看报告",
    hint: "总结与导出",
    icon: Sparkles,
    render: () => <ReportView />,
  },
];

const APP_VERSION = `v${appPackage.version}`;

export function HomePage() {
  const currentStep = useAppStore((s) => s.currentStep);
  const setStep = useAppStore((s) => s.setStep);
  const reset = useAppStore((s) => s.reset);
  const filePairs = useAppStore((s) => s.filePairs);
  const dbConfig = useAppStore((s) => s.dbConfig);
  const preCheckResults = useAppStore((s) => s.preCheckResults);
  const isLoading = useAppStore((s) => s.isLoading);
  const report = useAppStore((s) => s.report);

  // 全局订阅后端推送的加载事件（loading-progress / table-completed /
  // loading-completed / loading-error）并同步到 store。
  useLoadingEvents();

  const active = STEPS[currentStep] ?? STEPS[0];

  const canAdvance = useMemo(() => {
    switch (currentStep) {
      case 0:
        return filePairs.length > 0;
      case 1:
        return (
          !!dbConfig.host && !!dbConfig.port && !!dbConfig.database &&
          !!dbConfig.username
        );
      case 2: {
        if (preCheckResults.length === 0) return false;
        return !preCheckResults.some(
          (r) => !r.passed && r.severity === "error",
        );
      }
      case 3:
        return !!report || !isLoading;
      default:
        return false;
    }
  }, [currentStep, filePairs, dbConfig, preCheckResults, isLoading, report]);

  const isStepCompleted = useCallback((index: number): boolean => {
    switch (index) {
      case 0:
        return filePairs.length > 0;
      case 1:
        return !!dbConfig.host && !!dbConfig.port && !!dbConfig.database && !!dbConfig.username;
      case 2:
        return preCheckResults.length > 0 && !preCheckResults.some(r => !r.passed && r.severity === "error");
      case 3:
        return report !== null;
      case 4:
        return false;
      default:
        return false;
    }
  }, [filePairs, dbConfig, preCheckResults, report]);

  const goNext = () => {
    if (currentStep < STEPS.length - 1) setStep(currentStep + 1);
  };
  const goPrev = () => {
    if (currentStep > 0) setStep(currentStep - 1);
  };

  return (
    <div className="relative flex h-screen flex-col overflow-hidden">
      <Header onReset={reset} />

      <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col overflow-hidden px-6 pt-6">
        <Stepper current={currentStep} onJump={(i) => setStep(i)} isStepCompleted={isStepCompleted} />

        <Separator className="my-4" />

        {/* DESIGN.md: 内容卡片用 rounded-lg (12px)，奶油色背景 */}
        <div className="flex-1 overflow-hidden rounded-lg border bg-card p-6 md:p-8">
          <div className="h-full overflow-y-auto">
            <Suspense fallback={<div className="flex items-center justify-center py-20 text-sm text-muted-foreground">加载中...</div>}>
              {active.render()}
            </Suspense>
          </div>
        </div>

        {/* 底部导航 */}
        <div className="flex flex-wrap items-center justify-between gap-3 py-4">
          <Button
            variant="ghost"
            onClick={goPrev}
            disabled={currentStep === 0}
            className="gap-2"
          >
            <ArrowLeft className="h-4 w-4" />
            上一步
          </Button>

          <p className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
            {active.code} / {String(STEPS.length).padStart(2, "0")} ·{" "}
            {active.hint}
          </p>

          {currentStep < STEPS.length - 1 ? (
            <Button
              onClick={goNext}
              disabled={!canAdvance}
              className="gap-2"
            >
              下一步
              <ArrowRight className="h-4 w-4" />
            </Button>
          ) : (
            <Button onClick={reset} variant="outline" className="gap-2">
              <RefreshCcw className="h-4 w-4" />
              开始新任务
            </Button>
          )}
        </div>
      </main>
    </div>
  );
}

function Header({ onReset }: { onReset: () => void }) {
  return (
    <header
      data-tauri-drag-region
      className="sticky top-0 z-20 select-none border-b bg-background"
    >
      <div
        data-tauri-drag-region
        className="mx-auto flex h-10 w-full max-w-6xl items-center justify-between px-6"
      >
        <div data-tauri-drag-region className="flex items-center gap-2.5">
          <img
            src="/app-icon.png"
            alt=""
            data-tauri-drag-region
            className="pointer-events-none h-7 w-7 rounded-md shadow-sm"
          />
          <div data-tauri-drag-region className="leading-tight">
            <p data-tauri-drag-region className="text-sm font-semibold tracking-tight">
              GUT&nbsp;Loader
            </p>
            <p data-tauri-drag-region className="font-mono text-[9px] uppercase tracking-[0.2em] text-muted-foreground">
              Universal data ingestion · {APP_VERSION}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            onClick={onReset}
            className="h-7 gap-1.5 text-muted-foreground hover:text-foreground"
          >
            <RefreshCcw className="h-3 w-3" />
            重置
          </Button>
          <WindowControls />
        </div>
      </div>
    </header>
  );
}

/** 窗口控制按钮（最小化/最大化/关闭）- 全平台显示 */
function WindowControls() {
  const handleMinimize = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().minimize();
  };
  const handleToggleMaximize = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().toggleMaximize();
  };
  const handleClose = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().close();
  };

  return (
    <div className="flex items-center">
      <button
        type="button"
        onClick={handleMinimize}
        className="grid h-7 w-8 place-items-center text-muted-foreground transition-colors hover:bg-muted"
        title="最小化"
      >
        <svg width="10" height="1" viewBox="0 0 10 1" fill="currentColor">
          <rect width="10" height="1" />
        </svg>
      </button>
      <button
        type="button"
        onClick={handleToggleMaximize}
        className="grid h-7 w-8 place-items-center text-muted-foreground transition-colors hover:bg-muted"
        title="最大化"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
          <rect x="0.5" y="0.5" width="9" height="9" />
        </svg>
      </button>
      <button
        type="button"
        onClick={handleClose}
        className="grid h-7 w-8 place-items-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
        title="关闭"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.2">
          <path d="M1 1L9 9M9 1L1 9" />
        </svg>
      </button>
    </div>
  );
}

function Stepper({
  current,
  onJump,
  isStepCompleted,
}: {
  current: number;
  onJump: (index: number) => void;
  isStepCompleted: (index: number) => boolean;
}) {
  return (
    <ol className="grid grid-cols-5 gap-1.5 md:gap-2">
      {STEPS.map((s) => {
        const state = isStepCompleted(s.index)
          ? "done"
          : s.index === current
            ? "active"
            : "todo";
        const Icon = s.icon;
        return (
          <li key={s.code}>
            <button
              type="button"
              onClick={() => onJump(s.index)}
              className={cn(
                /* DESIGN.md: 步骤卡片用 rounded-lg (12px)，奶油色卡片背景 */
                "group flex w-full items-start gap-2.5 rounded-lg border bg-card px-3 py-2.5 text-left transition-colors md:px-4 md:py-3",
                /* 活动步骤：珊瑚色边框 + 焦点环 */
                state === "active" &&
                  "border-primary/80 shadow-sm ring-1 ring-primary/10",
                /* 完成步骤：success 绿色边框 + 浅绿背景 */
                state === "done" && "border-success/40 bg-success-light",
                state === "todo" && "opacity-80 hover:opacity-100",
              )}
            >
              <div
                className={cn(
                  "mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-md font-mono text-xs font-semibold transition-colors",
                  /* 完成：success 绿色填充 */
                  state === "done" &&
                    "bg-success text-white",
                  /* 活动：深色填充（使用 foreground） */
                  state === "active" &&
                    "bg-foreground text-background",
                  state === "todo" &&
                    "bg-muted text-muted-foreground",
                )}
              >
                {state === "done" ? (
                  <Check className="h-4 w-4" />
                ) : (
                  s.code
                )}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <Icon
                    className={cn(
                      "h-3.5 w-3.5",
                      state === "active"
                        ? "text-foreground"
                        : state === "done"
                          ? "text-success"
                          : "text-muted-foreground",
                    )}
                  />
                  <p
                    className={cn(
                      "truncate text-sm font-semibold",
                      state === "todo" && "text-muted-foreground",
                    )}
                  >
                    {s.title}
                  </p>
                </div>
                <p className="mt-0.5 hidden truncate text-xs text-muted-foreground md:block">
                  {s.hint}
                </p>
              </div>
              {state === "active" && (
                <ChevronRight className="hidden h-4 w-4 self-center text-muted-foreground md:block" />
              )}
            </button>
          </li>
        );
      })}
    </ol>
  );
}

export default HomePage;
