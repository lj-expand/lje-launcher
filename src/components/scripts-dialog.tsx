import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Download, ExternalLink, FolderOpen, Trash2 } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
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

interface RegistryScript {
  name: string;
  version: string;
  authors: string[];
  dependencies: string[];
  binaries: string[];
  repo: string;
  url: string;
  pushedAt: string;
  description: string;
}

interface RegistryData {
  generatedAt: string;
  scripts: RegistryScript[];
}

interface InstallResult {
  installed: string[];
  external: string[];
}

interface BinaryInfo {
  name: string;
  enabled: boolean;
  path: string;
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

const folderName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export function ScriptsDialog({ open, onOpenChange }: ScriptsDialogProps) {
  const [scripts, setScripts] = useState<ScriptInfo[] | null>(null);
  const [scriptsRoot, setScriptsRoot] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [pullingPath, setPullingPath] = useState<string | null>(null);
  // Which registry names are installed. State (not derived) so installs can
  // flip rows optimistically without waiting on a reload.
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());

  const [registry, setRegistry] = useState<RegistryData | null>(null);
  const [registryLoading, setRegistryLoading] = useState(false);
  const [installingName, setInstallingName] = useState<string | null>(null);
  const [binaries, setBinaries] = useState<BinaryInfo[] | null>(null);

  // Keep installed names in sync with the scripts list.
  useEffect(() => {
    setInstalledNames(new Set((scripts ?? []).map((s) => folderName(s.path))));
  }, [scripts]);

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
    if (open) {
      void load();
      void ensureRegistry();
      void loadBinaries();
    }
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

  const uninstallScript = async (script: ScriptInfo) => {
    const name = folderName(script.path);
    if (!confirm(`Uninstall '${name}'? This deletes the folder.`)) return;
    setError(null);
    try {
      await invoke("registry_uninstall", { name });
      setInstalledNames((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
      toast.success(`Uninstalled '${name}'`);
      await load();
    } catch (e) {
      toast.error(`Uninstall failed: ${errMsg(e)}`);
    }
  };

  // Fetches the registry if it hasn't been fetched yet. If the list fails, tries a refresh.
  const ensureRegistry = async () => {
    if (registry || registryLoading) return;
    setRegistryLoading(true);
    setError(null);
    try {
      setRegistry(await invoke<RegistryData>("registry_list"));
    } catch {
      try {
        setRegistry(await invoke<RegistryData>("registry_refresh"));
      } catch (e) {
        setError(`${errMsg(e)} — hit refresh to retry`);
      }
    } finally {
      setRegistryLoading(false);
    }
  };

  const refreshRegistry = async () => {
    setRegistryLoading(true);
    setError(null);
    try {
      setRegistry(await invoke<RegistryData>("registry_refresh"));
      toast.success("Registry updated");
    } catch (e) {
      toast.error(`Registry refresh failed: ${errMsg(e)}`);
    } finally {
      setRegistryLoading(false);
    }
  };

  const installScript = async (name: string) => {
    setInstallingName(name);
    setError(null);
    try {
      const result = await invoke<InstallResult>("registry_install", {
        name,
      });
      const top = result.installed[result.installed.length - 1] ?? name;
      const deps =
        result.installed.length > 1
          ? ` (deps: ${result.installed.slice(0, -1).join(", ")})`
          : "";
      // Flip rows immediately; the reload below confirms.
      setInstalledNames((prev) => {
        const next = new Set(prev);
        for (const n of result.installed) next.add(n);
        return next;
      });
      toast.success(`Installed ${top}${deps}`);
      if (result.external.length > 0) {
        toast.warning(
          `Not installed: ${result.external.join(", ")} — not in the registry`,
        );
      }
      await load();
    } catch (e) {
      toast.error(`Install failed: ${errMsg(e)}`);
    } finally {
      setInstallingName(null);
    }
  };

  const loadBinaries = async () => {
    try {
      setBinaries(await invoke<BinaryInfo[]>("list_binaries"));
    } catch {
      setBinaries([]);
    }
  };

  const toggleBinary = async (binary: BinaryInfo, enabled: boolean) => {
    // Optimistic update; revert if the backend rejects.
    setBinaries((prev) =>
      prev
        ? prev.map((b) => (b.path === binary.path ? { ...b, enabled } : b))
        : prev,
    );
    try {
      await invoke("set_binary_enabled", { path: binary.path, enabled });
    } catch {
      setBinaries((prev) =>
        prev
          ? prev.map((b) =>
              b.path === binary.path ? { ...b, enabled: !enabled } : b,
            )
          : prev,
      );
    }
  };

  const sortedRegistry = [...(registry?.scripts ?? [])].sort((a, b) =>
    b.pushedAt.localeCompare(a.pushedAt),
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Scripts</DialogTitle>
          <DialogDescription>
            {scripts
              ? `${scripts.length} script${scripts.length === 1 ? "" : "s"} found`
              : "loading…"}
          </DialogDescription>
        </DialogHeader>

        {error && <p className="text-xs text-destructive">{error}</p>}

        <Tabs defaultValue="installed">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="installed">Installed</TabsTrigger>
            <TabsTrigger value="registry">Registry</TabsTrigger>
            <TabsTrigger value="binaries">Binaries</TabsTrigger>
          </TabsList>

          <TabsContent value="installed">
            <ScrollArea className="h-96">
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
                              const depScript = scripts.find(
                                (s) => s.name === dep,
                              );
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
                          onClick={() => void uninstallScript(script)}
                          aria-label={`Uninstall ${script.name ?? script.path}`}
                          title="Uninstall"
                          className="cursor-pointer p-1 text-muted-foreground hover:text-destructive"
                        >
                          <Trash2 className="size-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void invoke("open_folder", {
                              path: script.path,
                            }).catch(console.error)
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

            <div className="mt-3 flex items-center justify-end gap-3">
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
          </TabsContent>

          <TabsContent value="registry">
            <ScrollArea className="h-96">
              <div className="flex flex-col gap-2 pr-3">
                {sortedRegistry.length === 0 ? (
                  <p className="py-8 text-center text-xs text-muted-foreground">
                    {registryLoading
                      ? "loading…"
                      : "registry not fetched yet - hit refresh"}
                  </p>
                ) : (
                  sortedRegistry.map((entry) => {
                    const installed = installedNames.has(entry.name);
                    return (
                      <div
                        key={entry.name}
                        className="flex items-start justify-between gap-3 rounded-[10px] bg-[#252525] px-3.5 py-2.5"
                      >
                        <div className="min-w-0">
                          <div className="flex items-center gap-2">
                            <p className="truncate text-sm font-medium text-foreground">
                              {entry.name}
                            </p>
                            <span className="shrink-0 text-[10px] text-muted-foreground">
                              v{entry.version}
                            </span>
                            <a
                              href={entry.url}
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              <ExternalLink className="size-2.5 cursor-pointer text-muted-foreground hover:text-foreground" />
                            </a>
                          </div>
                          <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
                            {entry.description}
                          </p>
                          <p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">
                            {entry.repo}
                            {entry.authors.length > 0
                              ? ` · ${entry.authors.join(", ")}`
                              : ""}
                            {entry.binaries.length > 0
                              ? ` · needs: ${entry.binaries.join(", ")}`
                              : ""}
                          </p>
                        </div>
                        <div className="flex shrink-0 items-center gap-2">
                          {installed ? (
                            <Button
                              size="sm"
                              variant="secondary"
                              disabled
                              className="shrink-0"
                            >
                              Installed
                            </Button>
                          ) : (
                            <Button
                              size="sm"
                              onClick={() => void installScript(entry.name)}
                              disabled={installingName === entry.name}
                              className="shrink-0"
                            >
                              {installingName === entry.name
                                ? "Installing…"
                                : "Install"}
                            </Button>
                          )}
                        </div>
                      </div>
                    );
                  })
                )}
              </div>
            </ScrollArea>

            <div className="mt-3 flex items-center justify-end gap-3">
              {registry && (
                <span className="text-[10px] text-muted-foreground">
                  generated {registry.generatedAt.slice(0, 10)}
                </span>
              )}
              <button
                type="button"
                onClick={() => void refreshRegistry()}
                disabled={registryLoading}
                className="cursor-pointer text-[11px] text-[#dda770] hover:underline disabled:opacity-50"
              >
                {registryLoading ? "refreshing…" : "refresh"}
              </button>
            </div>
          </TabsContent>

          <TabsContent value="binaries">
            <ScrollArea className="h-96">
              <div className="flex flex-col gap-2 pr-3">
                {binaries === null ? (
                  <p className="py-8 text-center text-xs text-muted-foreground">
                    loading…
                  </p>
                ) : binaries.length === 0 ? (
                  <p className="py-8 text-center text-xs text-muted-foreground">
                    no binaries found in ~/.lje/binaries
                  </p>
                ) : (
                  binaries.map((binary) => (
                    <div
                      key={binary.path}
                      className="flex items-center justify-between gap-3 rounded-[10px] bg-[#252525] px-3.5 py-2.5"
                    >
                      <div className="min-w-0">
                        <p className="truncate font-mono text-sm font-medium text-foreground">
                          {binary.name}
                        </p>
                        <p className="truncate font-mono text-[10px] text-muted-foreground">
                          {binary.path.split(/[\\/]/).pop()}
                        </p>
                      </div>
                      <Checkbox
                        checked={binary.enabled}
                        onCheckedChange={(checked) =>
                          void toggleBinary(binary, checked)
                        }
                        aria-label={`toggle ${binary.name}`}
                        className="shrink-0"
                      />
                    </div>
                  ))
                )}
              </div>
            </ScrollArea>

            <div className="mt-3 flex items-center justify-end gap-3">
              <span className="text-[10px] text-muted-foreground">
                {binaries?.some((b) => !b.enabled)
                  ? `${binaries.filter((b) => !b.enabled).length} disabled`
                  : "all binaries active"}
              </span>
              <button
                type="button"
                onClick={() => void loadBinaries()}
                className="cursor-pointer text-[11px] text-[#dda770] hover:underline"
              >
                refresh
              </button>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
