import { useState } from "react";
import {
  Bug,
  ChevronDown,
  ChevronUp,
  Loader2,
  PlugZap,
  Save,
  Settings2,
  Sparkles,
  Wand2,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { LLMConfig } from "@/components/LLMConfig";
import { useAppStore } from "@/stores/appStore";
import { useTauriCommands } from "@/hooks/useTauriCommands";
import {
  DB_TYPE_DEFAULT_PORT,
  DB_TYPE_LABEL,
  DB_TYPE_OPTIONS,
  DM_SUPPORTED_ON_CURRENT_PLATFORM,
  DM_UNSUPPORTED_MESSAGE,
  type DbType,
} from "@/lib/types";

/**
 * 步骤 2：目标数据库连接配置 + LLM 智能识别填充。
 */
export function DatabaseConfig() {
  const dbConfig = useAppStore((s) => s.dbConfig);
  const setDbConfig = useAppStore((s) => s.setDbConfig);
  const llmConfig = useAppStore((s) => s.llmConfig);
  const savedDbConfigs = useAppStore((s) => s.savedDbConfigs);
  const saveDbConfig = useAppStore((s) => s.saveDbConfig);
  const deleteSavedDbConfig = useAppStore((s) => s.deleteSavedDbConfig);
  const loadSavedDbConfig = useAppStore((s) => s.loadSavedDbConfig);
  const { testConnection, parseDbInfo, getAppLogs, clearAppLogs } = useTauriCommands();

  const [testing, setTesting] = useState(false);
  const [parsing, setParsing] = useState(false);
  const [showAi, setShowAi] = useState(true);
  const [aiText, setAiText] = useState("");
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [configName, setConfigName] = useState("");
  const [showDebug, setShowDebug] = useState(false);
  const [appLogs, setAppLogs] = useState<string[]>([]);
  const [lastError, setLastError] = useState<string>("");

  const onTypeChange = (val: string) => {
    const next = val as DbType;
    setDbConfig({ db_type: next, port: DB_TYPE_DEFAULT_PORT[next] });
  };

  const loadLogs = async () => {
    const logs = await getAppLogs();
    setAppLogs(logs);
  };

  const handleClearLogs = async () => {
    await clearAppLogs();
    setAppLogs([]);
  };

  const handleTest = async () => {
    setTesting(true);
    setLastError("");
    try {
      const result = await testConnection(dbConfig);
      if (result.ok) {
        toast.success("数据库连接成功", {
          description: `${DB_TYPE_LABEL[dbConfig.db_type]} @ ${dbConfig.host}:${dbConfig.port}`,
        });
      } else {
        const errMsg = result.error || "未知错误";
        setLastError(errMsg);
        toast.error("数据库连接失败", {
          description: errMsg,
        });
      }
    } finally {
      setTesting(false);
    }
  };

  const handleAiParse = async () => {
    if (!aiText.trim()) {
      toast.warning("请粘贴包含连接信息的文本");
      return;
    }
    setParsing(true);
    try {
      const parsed = await parseDbInfo(aiText, llmConfig);
      const keys = Object.keys(parsed) as (keyof typeof parsed)[];
      if (keys.length === 0) {
        toast.error("未能从文本中识别到有效字段");
        return;
      }
      setDbConfig(parsed);
      toast.success("已自动填充", {
        description: `识别字段：${keys.join(", ")}`,
      });
    } finally {
      setParsing(false);
    }
  };

  const handleLoadConfig = (id: string) => {
    loadSavedDbConfig(id);
    const cfg = savedDbConfigs.find((c) => c.id === id);
    toast.success("已加载配置", {
      description: cfg ? cfg.name : "",
    });
  };

  const handleSaveConfig = () => {
    if (!configName.trim()) {
      toast.warning("请输入配置名称");
      return;
    }
    saveDbConfig(configName.trim());
    toast.success("配置已保存", { description: configName.trim() });
    setConfigName("");
    setSaveDialogOpen(false);
  };

  const handleDeleteConfig = (id: string, e: React.PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    const cfg = savedDbConfigs.find((c) => c.id === id);
    deleteSavedDbConfig(id);
    toast.success("已删除配置", { description: cfg?.name });
  };

  return (
    <div className="space-y-4">
      <header className="space-y-1">
        <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
          STEP / 02 — Sink configuration
        </p>
        <h2 className="text-xl font-semibold tracking-tight">
          配置目标数据库
        </h2>
        <p className="text-xs text-muted-foreground max-w-2xl">
          所有连接凭据仅保存在本机内存中，不会持久化或上传。可使用 LLM
          直接从一段文本中识别填充字段。
        </p>
      </header>

      {/* 主连接表单 — DESIGN.md: 奶油卡片 rounded-lg */}
      <div className="rounded-lg border bg-card p-4">
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-2">
            {/* DESIGN.md: 图标用 primary（珊瑚色） */}
            <PlugZap className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold tracking-wide">连接参数</h3>
          </div>
          <div className="flex items-center gap-2">
            <Select onValueChange={handleLoadConfig}>
              <SelectTrigger className="h-8 w-[220px] text-xs">
                <SelectValue placeholder="加载已保存配置" />
              </SelectTrigger>
              <SelectContent>
                {savedDbConfigs.map((cfg) => (
                  <SelectItem key={cfg.id} value={cfg.id}>
                    <span className="flex w-full items-center justify-between gap-2">
                      <span className="truncate">
                        {cfg.name}
                        <span className="ml-1 text-muted-foreground">
                          {cfg.db_type} @ {cfg.host}:{cfg.port}
                        </span>
                      </span>
                      <button
                        type="button"
                        className="ml-1 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm hover:bg-destructive/20 hover:text-destructive"
                        onPointerDown={(e) => handleDeleteConfig(cfg.id, e)}
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              size="sm"
              variant="outline"
              onClick={() => setSaveDialogOpen(true)}
              className="gap-1.5"
            >
              <Save className="h-4 w-4" />
              保存
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={handleTest}
              disabled={testing}
              className="gap-2"
            >
              {testing ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <PlugZap className="h-4 w-4" />
              )}
              {testing ? "测试中…" : "测试连接"}
            </Button>
          </div>
        </div>

        <div className="grid gap-4 md:grid-cols-12">
          <div className="space-y-1 md:col-span-4">
            <Label>数据库类型</Label>
            <Select value={dbConfig.db_type} onValueChange={onTypeChange}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {DB_TYPE_OPTIONS.map(([k, label]) => (
                  <SelectItem key={k} value={k}>
                    <span>{label}</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {!DM_SUPPORTED_ON_CURRENT_PLATFORM && (
              <p className="text-xs text-muted-foreground">
                {DM_UNSUPPORTED_MESSAGE}
              </p>
            )}
          </div>

          <div className="space-y-1 md:col-span-5">
            <Label htmlFor="host">主机</Label>
            <Input
              id="host"
              value={dbConfig.host}
              onChange={(e) => setDbConfig({ host: e.target.value })}
              placeholder="127.0.0.1"
              className="font-mono"
            />
          </div>

          <div className="space-y-1 md:col-span-3">
            <Label htmlFor="port">端口</Label>
            <Input
              id="port"
              type="number"
              value={dbConfig.port}
              onChange={(e) =>
                setDbConfig({ port: Number(e.target.value) || 0 })
              }
              className="font-mono"
            />
          </div>

          <div className="space-y-1 md:col-span-6">
            <Label htmlFor="db">数据库 / 实例</Label>
            <Input
              id="db"
              value={dbConfig.database}
              onChange={(e) => setDbConfig({ database: e.target.value })}
              placeholder="gut_warehouse"
              className="font-mono"
            />
          </div>

          <div className="space-y-1 md:col-span-6">
            <Label htmlFor="schema">
              Schema <span className="text-muted-foreground">（可选）</span>
            </Label>
            <Input
              id="schema"
              value={dbConfig.schema ?? ""}
              onChange={(e) => setDbConfig({ schema: e.target.value })}
              placeholder="public"
              className="font-mono"
            />
          </div>

          <div className="space-y-1 md:col-span-6">
            <Label htmlFor="user">用户名</Label>
            <Input
              id="user"
              value={dbConfig.username}
              onChange={(e) => setDbConfig({ username: e.target.value })}
              placeholder="loader"
              className="font-mono"
            />
          </div>

          <div className="space-y-1 md:col-span-6">
            <Label htmlFor="pwd">密码</Label>
            <Input
              id="pwd"
              type="password"
              value={dbConfig.password}
              onChange={(e) => setDbConfig({ password: e.target.value })}
              placeholder="••••••••"
              className="font-mono"
            />
          </div>
        </div>
      </div>

      {/* 保存配置 Dialog */}
      <Dialog open={saveDialogOpen} onOpenChange={setSaveDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>保存数据库配置</DialogTitle>
            <DialogDescription>
              为当前连接参数命名以便日后快速加载。密码不会被保存。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5 py-1">
            <Label htmlFor="config-name">配置名称</Label>
            <Input
              id="config-name"
              value={configName}
              onChange={(e) => setConfigName(e.target.value)}
              placeholder="例：生产库-PostgreSQL"
              onKeyDown={(e) => e.key === "Enter" && handleSaveConfig()}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSaveDialogOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSaveConfig} disabled={!configName.trim()}>
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* LLM 智能填充 — DESIGN.md: rounded-lg 奶油卡片 */}
      <div className="overflow-hidden rounded-lg border bg-card">
        <div className="flex w-full items-center justify-between gap-3 px-4 py-3">
          <button
            type="button"
            onClick={() => setShowAi((s) => !s)}
            className="flex flex-1 items-center gap-2 text-left transition-colors"
          >
            {/* DESIGN.md: LLM 图标用 accent（琥珀色）浅底 */}
            <span className="grid h-8 w-8 place-items-center rounded-md bg-accent/10 text-accent">
              <Sparkles className="h-4 w-4" />
            </span>
            <span>
              <span className="block text-sm font-semibold">
                LLM 智能识别填充
              </span>
              <span className="text-xs text-muted-foreground">
                粘贴一段包含连接信息的文本，自动解析为表单字段
              </span>
            </span>
          </button>
          <div className="flex items-center gap-2">
            <Dialog>
              <DialogTrigger asChild>
                <Button
                  size="sm"
                  variant="outline"
                  className="relative gap-1.5"
                >
                  <Settings2 className="h-4 w-4" />
                  配置 LLM
                  {/* DESIGN.md: 未配置指示点用 accent（琥珀色） */}
                  {(!llmConfig.api_url || !llmConfig.model) && (
                    <span className="absolute -right-1 -top-1 h-2 w-2 rounded-full bg-accent ring-2 ring-background" />
                  )}
                </Button>
              </DialogTrigger>
              <DialogContent className="sm:max-w-2xl">
                <DialogHeader>
                  <DialogTitle>LLM 服务配置</DialogTitle>
                  <DialogDescription>
                    配置大语言模型 API 端点，用于智能识别数据库连接信息
                  </DialogDescription>
                </DialogHeader>
                <div className="py-1">
                  <LLMConfig />
                </div>
              </DialogContent>
            </Dialog>
            <button
              type="button"
              onClick={() => setShowAi((s) => !s)}
              className="grid h-8 w-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-muted/40"
              aria-label={showAi ? "折叠" : "展开"}
            >
              {showAi ? (
                <ChevronUp className="h-4 w-4" />
              ) : (
                <ChevronDown className="h-4 w-4" />
              )}
            </button>
          </div>
        </div>

        {showAi && (
          <div className="space-y-4 border-t px-4 py-4">
            <div className="space-y-1.5">
              <Label htmlFor="ai-text">连接信息文本</Label>
              <textarea
                id="ai-text"
                value={aiText}
                onChange={(e) => setAiText(e.target.value)}
                rows={5}
                className="block w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                placeholder={`例：\n生产库连接信息：\n  类型 PostgreSQL，地址 10.20.30.40:5432\n  数据库 ods，用户 loader，密码 P@ssw0rd\n  schema 为 raw`}
              />
            </div>

            <div className="flex items-center justify-end gap-3">
              {(!llmConfig.api_url || !llmConfig.model) && (
                /* DESIGN.md: 提示文字用 accent（琥珀色） */
                <span className="text-xs text-accent">
                  请先配置 LLM API URL 和模型
                </span>
              )}
              <Button
                type="button"
                onClick={handleAiParse}
                disabled={
                  parsing ||
                  !aiText.trim() ||
                  !llmConfig.api_url ||
                  !llmConfig.model
                }
                className="gap-2"
              >
                {parsing ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Wand2 className="h-4 w-4" />
                )}
                智能识别
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* 调试日志面板 — DESIGN.md: rounded-lg 卡片 */}
      <div className="overflow-hidden rounded-lg border bg-card">
        <div className="flex w-full items-center justify-between gap-3 px-4 py-2">
          <button
            type="button"
            onClick={() => {
              setShowDebug((s) => !s);
              if (!showDebug) loadLogs();
            }}
            className="flex flex-1 items-center gap-2 text-left transition-colors"
          >
            <Bug className="h-3.5 w-3.5 text-muted-foreground/60" />
            <span className="text-xs text-muted-foreground/60">
              调试日志
            </span>
            {lastError && !showDebug && (
              <span className="inline-flex items-center rounded-full bg-destructive/10 px-1.5 py-0.5 text-[10px] text-destructive">
                有错误
              </span>
            )}
          </button>
          <div className="flex items-center gap-1">
            {showDebug && (
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={loadLogs}
                  className="h-6 px-2 text-xs text-muted-foreground"
                >
                  刷新
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={handleClearLogs}
                  className="h-6 px-2 text-xs text-muted-foreground"
                >
                  清空
                </Button>
              </>
            )}
            <button
              type="button"
              onClick={() => {
                setShowDebug((s) => !s);
                if (!showDebug) loadLogs();
              }}
              className="grid h-6 w-6 place-items-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted/40"
              aria-label={showDebug ? "折叠" : "展开"}
            >
              {showDebug ? (
                <ChevronUp className="h-3 w-3" />
              ) : (
                <ChevronDown className="h-3 w-3" />
              )}
            </button>
          </div>
        </div>

        {showDebug && (
          <div className="border-t px-4 py-3 space-y-3">
            {/* 最近一次连接错误 */}
            {lastError && (
              <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2">
                <p className="text-xs font-medium text-destructive mb-1">
                  最近连接错误
                </p>
                <pre className="whitespace-pre-wrap break-all text-xs text-destructive/80 font-mono">
                  {lastError}
                </pre>
              </div>
            )}

            {/* 应用日志 — DESIGN.md: 深色表面面板 */}
            <div>
              <p className="text-xs font-medium text-muted-foreground mb-1.5">
                应用运行日志（最近 {appLogs.length} 条）
              </p>
              {/* DESIGN.md: code-window-card 深色海军底 */}
              <div className="max-h-64 overflow-auto rounded-md border bg-surface-dark p-2">
                {appLogs.length === 0 ? (
                  <p className="text-xs text-on-dark-soft font-mono">
                    暂无日志记录
                  </p>
                ) : (
                  appLogs.map((line, i) => {
                    const isError = line.includes("ERROR");
                    const isWarn = line.includes("WARN");
                    return (
                      <div
                        key={i}
                        className={`font-mono text-[11px] leading-relaxed break-all ${
                          isError
                            ? "text-destructive"
                            : isWarn
                              ? "text-accent"
                              : "text-on-dark-soft"
                        }`}
                      >
                        {line}
                      </div>
                    );
                  })
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
