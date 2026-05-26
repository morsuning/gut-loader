import { useCallback, useMemo } from "react";
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
import { ReportView } from "@/components/ReportView";

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
    <div className="relative min-h-screen overflow-hidden">
      {/* 背景纹理：极淡的网格 + 顶部光晕 */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-grid-pattern opacity-[0.18]"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 -top-32 -z-10 h-[420px] bg-gradient-to-b from-amber-200/30 via-transparent to-transparent dark:from-amber-500/10"
      />

      <Header onReset={reset} />

      <main className="mx-auto w-full max-w-6xl px-6 pb-24 pt-10">
        <Stepper current={currentStep} onJump={(i) => setStep(i)} isStepCompleted={isStepCompleted} />

        <Separator className="my-8" />

        <div className="rounded-2xl border bg-card/60 p-6 shadow-sm backdrop-blur md:p-10">
          {active.render()}
        </div>

        {/* 底部导航 */}
        <div className="mt-10 flex flex-wrap items-center justify-between gap-3">
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
    <header className="sticky top-0 z-20 border-b bg-background/85 backdrop-blur">
      <div className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-6">
        <div className="flex items-center gap-3">
          <img
            src="/app-icon.png"
            alt=""
            className="h-8 w-8 rounded-md shadow-sm"
          />
          <div className="leading-tight">
            <p className="text-sm font-semibold tracking-tight">
              GUT&nbsp;Loader
            </p>
            <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
              Universal data ingestion · {APP_VERSION}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={onReset}
            className="gap-1.5 text-muted-foreground hover:text-foreground"
          >
            <RefreshCcw className="h-3.5 w-3.5" />
            重置流程
          </Button>
        </div>
      </div>
    </header>
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
    <ol className="grid grid-cols-5 gap-2 md:gap-3">
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
                "group flex w-full items-start gap-3 rounded-xl border bg-card/60 px-3 py-3 text-left transition-all md:px-4 md:py-4",
                state === "active" &&
                  "border-foreground/80 bg-card shadow-sm ring-1 ring-foreground/10",
                state === "done" && "border-emerald-500/40 bg-emerald-500/5",
                state === "todo" && "opacity-80 hover:opacity-100",
              )}
            >
              <div
                className={cn(
                  "mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-md font-mono text-xs font-semibold transition-colors",
                  state === "done" &&
                    "bg-emerald-500 text-white dark:bg-emerald-500/90",
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
                          ? "text-emerald-600 dark:text-emerald-400"
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
