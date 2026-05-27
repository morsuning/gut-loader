import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  FolderOpen,
  Folder,
  ScanLine,
  FileText,
  FileArchive,
  Loader2,
  Inbox,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useAppStore } from "@/stores/appStore";
import { useTauriCommands } from "@/hooks/useTauriCommands";

/**
 * 步骤 1：选择目录并扫描 GUT 文件对（dat.gz + flg）。
 */
export function FileSelector() {
  const { scanDirectory } = useTauriCommands();
  const selectedDirectory = useAppStore((s) => s.selectedDirectory);
  const setSelectedDirectory = useAppStore((s) => s.setSelectedDirectory);
  const filePairs = useAppStore((s) => s.filePairs);
  const setFilePairs = useAppStore((s) => s.setFilePairs);

  const [scanning, setScanning] = useState(false);

  const handlePickDir = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择 GUT 数据目录",
      });
      if (typeof picked === "string" && picked) {
        setSelectedDirectory(picked);
        setFilePairs([]);
      }
    } catch (e) {
      console.error(e);
      toast.error("无法打开目录选择器", {
        description: "请确认已授予应用 dialog 权限",
      });
    }
  };

  const handleScan = async () => {
    if (!selectedDirectory) {
      toast.warning("请先选择目录");
      return;
    }
    setScanning(true);
    try {
      const pairs = await scanDirectory(selectedDirectory);
      setFilePairs(pairs);
      if (pairs.length === 0) {
        toast.info("未发现 GUT 文件对", {
          description: "请确认目录中存在配对的 .flg 与 .dat.gz",
        });
      } else {
        toast.success(`已识别 ${pairs.length} 个文件对`);
      }
    } finally {
      setScanning(false);
    }
  };

  return (
    <div className="space-y-4">
      <header className="space-y-1">
        <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
          STEP / 01 — Source ingestion
        </p>
        {/* DESIGN.md: 标题用正文 sans 字体，tracking-tight */}
        <h2 className="text-xl font-semibold tracking-tight">
          指向待入库的 GUT 数据目录
        </h2>
        <p className="text-xs text-muted-foreground max-w-2xl">
          扫描器会在指定目录中识别配对的{" "}
          <code className="font-mono text-foreground">.flg</code> 元数据与{" "}
          <code className="font-mono text-foreground">.dat.gz</code> 数据文件，
          并按表名 / 日期 / 序号自动归组。
        </p>
      </header>

      {/* DESIGN.md: 虚线边框区域，奶油色半透明背景 */}
      <div className="rounded-lg border border-dashed border-border bg-card/50 p-4">
        <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            {/* DESIGN.md: 图标区域用 primary 色（珊瑚色）浅背景 */}
            <div className="grid h-11 w-11 shrink-0 place-items-center rounded-md bg-primary/10 text-primary">
              <Folder className="h-5 w-5" />
            </div>
            <div className="min-w-0">
              <p className="text-xs font-mono uppercase tracking-wider text-muted-foreground">
                Selected directory
              </p>
              <p className="mt-1 truncate font-mono text-sm">
                {selectedDirectory || (
                  <span className="text-muted-foreground/60">
                    尚未选择目录…
                  </span>
                )}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handlePickDir}
              className="gap-2"
            >
              <FolderOpen className="h-4 w-4" />
              选择目录
            </Button>
            <Button
              size="sm"
              onClick={handleScan}
              disabled={!selectedDirectory || scanning}
              className="gap-2"
            >
              {scanning ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <ScanLine className="h-4 w-4" />
              )}
              {scanning ? "扫描中…" : "开始扫描"}
            </Button>
          </div>
        </div>
      </div>

      <section>
        <div className="flex items-baseline justify-between">
          <h3 className="text-sm font-semibold tracking-wide">扫描结果</h3>
          <span className="font-mono text-xs text-muted-foreground">
            {filePairs.length.toString().padStart(3, "0")} / pairs
          </span>
        </div>

        {filePairs.length === 0 ? (
          <EmptyResult scanned={!!selectedDirectory && !scanning} />
        ) : (
          /* DESIGN.md: 表格容器用 rounded-lg (12px) */
          <ScrollArea className="mt-2 h-[280px] rounded-lg border bg-card">
            <table className="w-full text-sm">
              <thead className="sticky top-0 z-10 bg-card">
                <tr className="border-b text-left text-xs font-mono uppercase tracking-wider text-muted-foreground">
                  <th className="px-4 py-3 font-medium">#</th>
                  <th className="px-4 py-3 font-medium">表名</th>
                  <th className="px-4 py-3 font-medium">日期 / 序号</th>
                  <th className="px-4 py-3 font-medium">FLG</th>
                  <th className="px-4 py-3 font-medium">DAT</th>
                  <th className="px-4 py-3 text-right font-medium">预估行数</th>
                </tr>
              </thead>
              <tbody>
                {filePairs.map((p, idx) => (
                  <tr
                    key={`${p.table_name}-${p.date}-${p.sequence}`}
                    className="border-b last:border-0 transition-colors hover:bg-muted/40"
                  >
                    <td className="px-4 py-3 font-mono text-xs text-muted-foreground">
                      {(idx + 1).toString().padStart(2, "0")}
                    </td>
                    <td className="px-4 py-3">
                      <Badge variant="secondary" className="font-mono">
                        {p.table_name}
                      </Badge>
                    </td>
                    <td className="px-4 py-3 font-mono text-xs text-muted-foreground">
                      {p.date}.{p.time}.{p.sequence}
                    </td>
                    <td className="px-4 py-3">
                      <FilePathCell icon="flg" path={p.flg_path} />
                    </td>
                    <td className="px-4 py-3">
                      <FilePathCell icon="dat" path={p.dat_path} />
                    </td>
                    <td className="px-4 py-3 text-right font-mono tabular-nums">
                      {p.estimated_rows != null
                        ? p.estimated_rows.toLocaleString()
                        : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </ScrollArea>
        )}
      </section>
    </div>
  );
}

function FilePathCell({ icon, path }: { icon: "flg" | "dat"; path: string }) {
  const Icon = icon === "flg" ? FileText : FileArchive;
  const filename = path.split(/[\\/]/).pop() ?? path;
  return (
    <div className="flex items-center gap-2 font-mono text-xs">
      <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate" title={path}>
        {filename}
      </span>
    </div>
  );
}

function EmptyResult({ scanned }: { scanned: boolean }) {
  return (
    <div className="mt-2 grid place-items-center rounded-lg border border-dashed bg-muted/30 px-6 py-12 text-center">
      <Inbox className="mb-2 h-7 w-7 text-muted-foreground/60" />
      <p className="text-sm font-medium text-muted-foreground">
        {scanned ? "目录中暂无识别到的 GUT 文件对" : "尚未扫描"}
      </p>
      <p className="mt-1 max-w-sm text-xs text-muted-foreground/70">
        请确保目录中包含成对的{" "}
        <code className="font-mono">tableName.YYYYMMDD.HHMMSS.NNNN.flg</code>{" "}
        与同名 <code className="font-mono">.dat.gz</code> 文件。
      </p>
    </div>
  );
}
