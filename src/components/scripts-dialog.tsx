import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, FolderOpen } from "lucide-react";

import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

interface ScriptGitInfo {
  commit: string | null;
  branch: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
}

interface ScriptInfo {
  name: string | null;
  author: string | null;
  version: string | null;
  url: string | null;
  dependencies: string[];
  enabled: boolean;
  path: string;
  isGit: boolean;
  gitInfo: ScriptGitInfo | null;
}

interface ScriptsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function errMsg(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function ScriptsDialog({ open, onOpenChange }: ScriptsDialogProps) {
  const [scripts, setScripts] = useState<ScriptInfo[] | null>(null);
  const [scriptsRoot, setScriptsRoot] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [pullingPath, setPullingPath] = useState<string | null>(null);

  const load = async () => {
    try {
      setScripts(await invoke<ScriptInfo[]>("list_scripts"));
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
    try {
      setScriptsRoot(await invoke<string>("scripts_dir"));
    } catch {
      // folder button stays inert
    }
  };

  useEffect(() => {
    if (open) void load();
  }, [open]);

  const toggle = async (script: ScriptInfo, enabled: boolean) => {
    // Optimistic update; revert if the backend rejects.
    setScripts((prev) =>
      prev
        ? prev.map((s) => (s.path === script.path ? { ...s, enabled } : s))
        : prev,
    );
    try {
      await invoke("set_script_enabled", { path: script.path, enabled });
    } catch {
      setScripts((prev) =>
        prev
          ? prev.map((s) =>
              s.path === script.path ? { ...s, enabled: !enabled } : s,
            )
          : prev,
      );
    }
  };

  const applyInfo = (path: string, info: ScriptGitInfo | null) =>
    setScripts((prev) =>
      prev
        ? prev.map((s) => (s.path === path ? { ...s, gitInfo: info } : s))
        : prev,
    );

  // Fetches every git-managed script in parallel.
  const checkUpdates = async () => {
    if (!scripts) return;
    setChecking(true);
    setError(null);
    try {
      const results = await Promise.all(
        scripts
          .filter((s) => s.isGit)
          .map(async (s) => {
            try {
              const info = await invoke<ScriptGitInfo>("script_check", {
                path: s.path,
              });
              return { path: s.path, info, error: null as string | null };
            } catch (e) {
              return { path: s.path, info: null, error: errMsg(e) };
            }
          }),
      );
      for (const r of results) applyInfo(r.path, r.info);
      const firstError = results.find((r) => r.error)?.error;
      if (firstError) setError(firstError);
    } finally {
      setChecking(false);
    }
  };

  const pull = async (script: ScriptInfo) => {
    setPullingPath(script.path);
    setError(null);
    try {
      const info = await invoke<ScriptGitInfo>("script_pull", {
        path: script.path,
      });
      applyInfo(script.path, info);
    } catch (e) {
      setError(`pull failed: ${errMsg(e)}`);
    } finally {
      setPullingPath(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Scripts</DialogTitle>
          <DialogDescription>
            {scripts
              ? `${scripts.length} script${scripts.length === 1 ? "" : "s"} found`
              : "loading…"}
          </DialogDescription>
        </DialogHeader>

        {error && <p className="text-xs text-destructive">{error}</p>}

        <ScrollArea className="h-80">
          <div className="flex flex-col gap-2 pr-3">
            {scripts === null ? (
              <p className="py-8 text-center text-xs text-muted-foreground">
                loading…
              </p>
            ) : scripts.length === 0 ? (
              <p className="py-8 text-center text-xs text-muted-foreground">
                no scripts found in ~/.lje/scripts
              </p>
            ) : (
              scripts.map((script) => (
                <div
                  key={script.path}
                  className="flex items-start justify-between gap-3 rounded-[10px] bg-[#252525] px-3.5 py-2.5"
                >
                  <div className="min-w-0">
                    {script.url ? (
                      <a
                        href={script.url}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        <p className="truncate text-sm font-medium text-accent hover:underline">
                          {script.name ?? "unknown"}
                        </p>
                      </a>
                    ) : (
                      <p className="truncate text-sm font-medium text-foreground">
                        {script.name ?? "unknown"}
                      </p>
                    )}
                    <p className="text-xs text-muted-foreground">
                      {script.author ?? "unknown"}
                      {script.version ? ` · ${script.version}` : ""}
                    </p>
                    {script.isGit && script.gitInfo && (
                      <div className="mt-0.5 flex flex-wrap items-center gap-1.5">
                        <span className="font-mono text-[10px] text-muted-foreground">
                          {script.gitInfo.branch ?? "?"} ·{" "}
                          {script.gitInfo.commit ?? "?"}
                        </span>
                        {script.gitInfo.dirty && (
                          <span className="text-[10px] text-[#dda770]">
                            modified
                          </span>
                        )}
                        {script.gitInfo.behind > 0 && (
                          <span className="text-[10px] text-[#dda770]">
                            ↓ {script.gitInfo.behind} behind
                          </span>
                        )}
                      </div>
                    )}
                    {script.dependencies.length > 0 && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {script.dependencies.map((dep) => {
                          const depScript = scripts.find((s) => s.name === dep);
                          const isDisabled =
                            depScript !== undefined && !depScript.enabled;
                          return (
                            <Badge
                              key={dep}
                              variant="secondary"
                              title={isDisabled ? `${dep} (disabled)` : dep}
                              className={cn(
                                "px-1.5 py-px text-[10px]",
                                isDisabled &&
                                  "border border-[#dda770]/30 bg-[#dda770]/10 text-[#dda770]",
                              )}
                            >
                              {dep}
                            </Badge>
                          );
                        })}
                      </div>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {script.isGit &&
                      script.gitInfo &&
                      script.gitInfo.behind > 0 && (
                        <button
                          type="button"
                          onClick={() => void pull(script)}
                          disabled={pullingPath === script.path}
                          aria-label={`Pull ${script.name ?? script.path}`}
                          title={`Pull (${script.gitInfo.behind} behind)`}
                          className="cursor-pointer p-1 text-[#dda770] hover:text-foreground disabled:opacity-50"
                        >
                          <Download className="size-3.5" />
                        </button>
                      )}
                    <button
                      type="button"
                      onClick={() =>
                        void invoke("open_folder", { path: script.path }).catch(
                          console.error,
                        )
                      }
                      aria-label={`Open folder of ${script.name ?? script.path}`}
                      title="Open script folder"
                      className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                    >
                      <FolderOpen className="size-3.5" />
                    </button>
                    <Checkbox
                      checked={script.enabled}
                      onCheckedChange={(checked) =>
                        void toggle(script, checked)
                      }
                      aria-label={`toggle ${script.name ?? script.path}`}
                      className="shrink-0"
                    />
                  </div>
                </div>
              ))
            )}
          </div>
        </ScrollArea>

        <div className="flex items-center justify-end gap-3">
          {scripts?.some((s) => s.isGit) && (
            <button
              type="button"
              onClick={() => void checkUpdates()}
              disabled={checking}
              className="cursor-pointer text-[11px] text-[#dda770] hover:underline disabled:opacity-50"
            >
              {checking ? "checking…" : "check for updates"}
            </button>
          )}
          <button
            type="button"
            onClick={() =>
              scriptsRoot &&
              void invoke("open_folder", { path: scriptsRoot }).catch(
                console.error,
              )
            }
            className="cursor-pointer text-[11px] text-[#dda770] hover:underline"
          >
            open folder
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
