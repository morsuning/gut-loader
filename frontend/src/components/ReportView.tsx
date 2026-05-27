import { useMemo } from "react";
import {
  Award,
  Database,
  Download,
  Gauge,
  ListChecks,
  Timer,
} from "lucide-react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useAppStore } from "@/stores/appStore";
import type { LoadReport } from "@/lib/types";
import { cn } from "@/lib/utils";

/**
 * 步骤 5：入库报告。汇总卡片 + 详情表格 + 性能图表 + JSON 导出。
 */
export function ReportView() {
  const report = useAppStore((s) => s.report);

  if (!report) {
    return (
      <div className="space-y-4">
        <header className="space-y-1">
          <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
            STEP / 05 — Load report
          </p>
          <h2 className="text-xl font-semibold tracking-tight">入库报告</h2>
        </header>
        <div className="grid place-items-center rounded-lg border border-dashed bg-muted/30 px-6 py-16 text-center">
          <Award className="mb-3 h-8 w-8 text-muted-foreground/60" />
          <p className="text-sm font-medium text-muted-foreground">
            暂无报告数据
          </p>
          <p className="mt-1 text-xs text-muted-foreground/70">
            完成步骤 4 的加载任务后，报告将自动出现在此处。
          </p>
        </div>
      </div>
    );
  }

  return <ReportContent report={report} />;
}

function ReportContent({ report }: { report: LoadReport }) {
  const successRatePct = Math.round(report.success_rate * 100);

  const speedData = useMemo(
    () =>
      report.table_reports.map((t) => ({
        name: t.table_name,
        speed: Math.round(t.speed),
      })),
    [report],
  );

  const pieData = useMemo(
    /* DESIGN.md: 成功用 success 绿，失败用 destructive 红 */
    () => [
      { name: "成功", value: report.success_rows, color: "hsl(var(--success))" },
      { name: "失败", value: report.failed_rows, color: "hsl(var(--destructive))" },
    ],
    [report],
  );

  const handleExport = () => {
    const blob = new Blob([JSON.stringify(report, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `gut-load-report-${Date.now()}.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div className="space-y-1">
          <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
            STEP / 05 — Load report
          </p>
          <h2 className="text-xl font-semibold tracking-tight">入库报告</h2>
          <p className="text-xs text-muted-foreground max-w-2xl">
            总耗时 {formatMs(report.total_elapsed_ms)} · 平均速率{" "}
            <span className="font-mono">
              {Math.round(report.avg_speed).toLocaleString()}
            </span>{" "}
            rows/s
          </p>
        </div>
        <Button variant="outline" onClick={handleExport} className="gap-2">
          <Download className="h-4 w-4" />
          导出 JSON
        </Button>
      </header>

      {/* 汇总四联卡 */}
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <SummaryCard
          icon={Database}
          label="总表数"
          value={report.total_tables.toLocaleString()}
          unit="tables"
        />
        <SummaryCard
          icon={ListChecks}
          label="总记录数"
          value={report.total_rows.toLocaleString()}
          unit="rows"
        />
        <SummaryCard
          icon={Award}
          label="成功率"
          value={`${successRatePct}%`}
          accent={
            successRatePct >= 99
              ? "success"
              : successRatePct >= 90
                ? "accent"
                : "destructive"
          }
        />
        <SummaryCard
          icon={Gauge}
          label="平均速率"
          value={Math.round(report.avg_speed).toLocaleString()}
          unit="rows/s"
          accent="accent"
        />
      </div>

      {/* 总耗时大数 */}
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-card p-4">
        <div className="flex items-center gap-3">
          {/* DESIGN.md: 图标用 primary（珊瑚色）浅底 */}
          <div className="grid h-10 w-10 place-items-center rounded-md bg-primary/10 text-primary">
            <Timer className="h-5 w-5" />
          </div>
          <div>
            <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-muted-foreground">
              Total elapsed
            </p>
            <p className="font-mono text-xl font-semibold tabular-nums">
              {formatMs(report.total_elapsed_ms)}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-3 text-right">
          <div>
            <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-muted-foreground">
              Success / Failed
            </p>
            <p className="font-mono text-base">
              {/* DESIGN.md: 成功用 success 绿 */}
              <span className="font-semibold text-success tabular-nums">
                {report.success_rows.toLocaleString()}
              </span>
              <span className="mx-1 text-muted-foreground">/</span>
              <span
                className={cn(
                  "font-semibold tabular-nums",
                  report.failed_rows > 0
                    ? "text-destructive"
                    : "text-muted-foreground",
                )}
              >
                {report.failed_rows.toLocaleString()}
              </span>
            </p>
          </div>
        </div>
      </div>

      {/* 详情表格 */}
      <section>
        <h3 className="mb-2 text-sm font-semibold tracking-wide">每表详情</h3>
        <div className="overflow-hidden rounded-lg border bg-card">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/30 text-left text-xs font-mono uppercase tracking-wider text-muted-foreground">
                <th className="px-3 py-2 font-medium">表名</th>
                <th className="px-3 py-2 text-right font-medium">行数</th>
                <th className="px-3 py-2 text-right font-medium">成功</th>
                <th className="px-3 py-2 text-right font-medium">失败</th>
                <th className="px-3 py-2 text-right font-medium">速率</th>
                <th className="px-3 py-2 text-right font-medium">耗时</th>
              </tr>
            </thead>
            <tbody>
              {report.table_reports.map((t) => {
                const failed = t.failed_count > 0;
                return (
                  <tr key={t.table_name} className="border-b last:border-0">
                    <td className="px-3 py-2">
                      <Badge variant="secondary" className="font-mono">
                        {t.table_name}
                      </Badge>
                      {t.errors.length > 0 && (
                        <span className="ml-2 font-mono text-xs text-destructive">
                          {t.errors.length} err
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-2 text-right font-mono tabular-nums">
                      {t.row_count.toLocaleString()}
                    </td>
                    {/* DESIGN.md: 成功用 success 绿 */}
                    <td className="px-3 py-2 text-right font-mono tabular-nums text-success">
                      {t.success_count.toLocaleString()}
                    </td>
                    <td
                      className={cn(
                        "px-3 py-2 text-right font-mono tabular-nums",
                        failed
                          ? "text-destructive"
                          : "text-muted-foreground",
                      )}
                    >
                      {t.failed_count.toLocaleString()}
                    </td>
                    <td className="px-3 py-2 text-right font-mono tabular-nums">
                      {Math.round(t.speed).toLocaleString()} r/s
                    </td>
                    <td className="px-3 py-2 text-right font-mono tabular-nums text-muted-foreground">
                      {formatMs(t.elapsed_ms)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </section>

      {/* 图表 — DESIGN.md: 图表柱用 primary（珊瑚色） */}
      <section className="grid gap-3 lg:grid-cols-5">
        <div className="rounded-lg border bg-card p-4 lg:col-span-3">
          <div className="mb-2 flex items-baseline justify-between">
            <h3 className="text-sm font-semibold tracking-wide">
              每表加载速率
            </h3>
            <span className="font-mono text-[11px] uppercase tracking-wider text-muted-foreground">
              rows / second
            </span>
          </div>
          <div className="h-48">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={speedData}>
                <CartesianGrid
                  vertical={false}
                  stroke="hsl(var(--border))"
                  strokeDasharray="3 3"
                />
                <XAxis
                  dataKey="name"
                  stroke="hsl(var(--muted-foreground))"
                  fontSize={11}
                  tickLine={false}
                />
                <YAxis
                  stroke="hsl(var(--muted-foreground))"
                  fontSize={11}
                  tickLine={false}
                  axisLine={false}
                />
                <Tooltip
                  cursor={{ fill: "hsl(var(--muted))" }}
                  contentStyle={{
                    background: "hsl(var(--popover))",
                    border: "1px solid hsl(var(--border))",
                    borderRadius: 8,
                    fontSize: 12,
                  }}
                />
                {/* DESIGN.md: 图表柱色用 primary（珊瑚色） */}
                <Bar
                  dataKey="speed"
                  fill="hsl(var(--primary))"
                  radius={[4, 4, 0, 0]}
                />
              </BarChart>
            </ResponsiveContainer>
          </div>
        </div>

        <div className="rounded-lg border bg-card p-4 lg:col-span-2">
          <div className="mb-2 flex items-baseline justify-between">
            <h3 className="text-sm font-semibold tracking-wide">
              成功 / 失败比例
            </h3>
            <span className="font-mono text-[11px] uppercase tracking-wider text-muted-foreground">
              total {report.total_rows.toLocaleString()}
            </span>
          </div>
          <div className="h-48">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Pie
                  data={pieData}
                  dataKey="value"
                  nameKey="name"
                  innerRadius={50}
                  outerRadius={80}
                  paddingAngle={2}
                >
                  {pieData.map((entry) => (
                    <Cell key={entry.name} fill={entry.color} />
                  ))}
                </Pie>
                <Tooltip
                  contentStyle={{
                    background: "hsl(var(--popover))",
                    border: "1px solid hsl(var(--border))",
                    borderRadius: 8,
                    fontSize: 12,
                  }}
                />
                <Legend
                  iconType="circle"
                  wrapperStyle={{ fontSize: 12 }}
                />
              </PieChart>
            </ResponsiveContainer>
          </div>
        </div>
      </section>
    </div>
  );
}

function SummaryCard({
  icon: Icon,
  label,
  value,
  unit,
  accent,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: string;
  unit?: string;
  accent?: "success" | "accent" | "destructive";
}) {
  /* DESIGN.md: 语义色映射 */
  const accentClass =
    accent === "success"
      ? "text-success"
      : accent === "accent"
        ? "text-accent"
        : accent === "destructive"
          ? "text-destructive"
          : "text-foreground";

  return (
    <div className="rounded-lg border bg-card p-4">
      <div className="flex items-center justify-between">
        <p className="text-[11px] font-mono uppercase tracking-[0.16em] text-muted-foreground">
          {label}
        </p>
        <Icon className="h-4 w-4 text-muted-foreground/60" />
      </div>
      <p
        className={cn(
          "mt-2 font-mono text-2xl font-semibold tabular-nums",
          accentClass,
        )}
      >
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

function formatMs(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  return `${m}m${rs.toString().padStart(2, "0")}s`;
}
