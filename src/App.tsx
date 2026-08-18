import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, Minus, Settings, Terminal, X } from "lucide-react";

import { ScriptsDialog } from "@/components/scripts-dialog";
import { SettingsDialog } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type LaunchState = "idle" | "injecting" | "success" | "fail";

interface LogEntry {
  time: string;
  message: string;
  success: boolean;
}

interface LogPayload {
  message: string;
  success: boolean;
}

interface UpdateInfo {
  current: string;
  latest: string;
  outOfDate: boolean;
}

const LED_CLASSES: Record<LaunchState, string> = {
  idle: "bg-gray-500",
  injecting: "bg-orange-500",
  success: "bg-[#639922]",
  fail: "bg-red-500",
};

function errMsg(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
}

function App() {
  const [scriptsOpen, setScriptsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [launchState, setLaunchState] = useState<LaunchState>("idle");
  const [injecting, setInjecting] = useState(false);
  const [gmodPath, setGmodPath] = useState<string | null>(null);
  const [version, setVersion] = useState("");
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const scrollAreaRef = useRef<HTMLDivElement | null>(null);
  const initRef = useRef(false);

  const addLog = useCallback((message: string, success = false) => {
    setLogs((prev) => [...prev, { time: timestamp(), message, success }]);
  }, []);

  // Init sequence + backend event listeners (guarded against StrictMode double-invoke).
  useEffect(() => {
    let disposed = false;
    const disposers: Array<() => void> = [];

    const wire = <T,>(event: string, handler: (payload: T) => void) => {
      listen<T>(event, (e) => {
        if (!disposed) handler(e.payload);
      }).then((unlisten) => {
        if (disposed) unlisten();
        else disposers.push(unlisten);
      });
    };

    wire<LogPayload>("log", (payload) =>
      addLog(payload.message, payload.success),
    );
    wire<LaunchState>("state", (state) => setLaunchState(state));

    if (!initRef.current) {
      initRef.current = true;
      addLog("initialized");
      setLaunchState("idle");

      void (async () => {
        try {
          const path = await invoke<string | null>("locate_gmod");
          if (path) {
            setGmodPath(path);
            addLog("gmod auto-located");
          }
        } catch {
          // ignore
        }

        try {
          setVersion(await invoke<string>("get_current_version"));
        } catch {
          // ignore
        }

        try {
          const update = await invoke<UpdateInfo>("check_update");
          if (update.outOfDate) setUpdateAvailable(true);
        } catch (error) {
          addLog(`failed to check for updates: ${errMsg(error)}`);
        }
      })();
    }

    return () => {
      disposed = true;
      disposers.forEach((unlisten) => unlisten());
    };
  }, [addLog]);

  // Kill the WebView2 native context menu (copy/paste/inspect junk).
  // WebView2 fires the DOM contextmenu first and honors preventDefault.
  useEffect(() => {
    const handler = (e: MouseEvent) => e.preventDefault();
    window.addEventListener("contextmenu", handler);
    return () => window.removeEventListener("contextmenu", handler);
  }, []);

  // The window starts hidden (tauri.conf.json "visible": false) so the user
  // never sees the transparent startup frame; reveal it after first paint.
  useEffect(() => {
    const timer = setTimeout(() => {
      const win = getCurrentWindow();
      void win.show();
      void win.setFocus();
    }, 150);
    return () => clearTimeout(timer);
  }, []);

  // Stick-to-bottom auto-scroll: only scroll when the log is already near the bottom.
  useEffect(() => {
    const viewport = scrollAreaRef.current?.querySelector<HTMLElement>(
      '[data-slot="scroll-area-viewport"]',
    );
    if (!viewport) return;
    const fromBottom =
      viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    if (fromBottom < 40) viewport.scrollTop = viewport.scrollHeight;
  }, [logs]);

  const handleChangePath = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "gmod", extensions: ["exe"] }],
        title: "Select gmod.exe",
      });
      if (typeof selected === "string") {
        setGmodPath(selected);
        addLog("target set");
      }
    } catch {
      // dialog dismissed or unavailable
    }
  }, [addLog]);

  const handleDownload = useCallback(
    async (onError?: (message: string) => void) => {
      try {
        addLog("downloading update...");
        await invoke("download_update");
        addLog("update downloaded!", true);
        setUpdateAvailable(false);
        setVersion(await invoke<string>("get_current_version"));
      } catch (error) {
        const message = errMsg(error);
        const friendly = message.includes("used by another process")
          ? "failed to update: please close gmod if it's open"
          : `failed to update: ${message}`;
        addLog(friendly);
        onError?.(friendly);
      }
    },
    [addLog],
  );

  const handleInject = useCallback(async () => {
    if (injecting) {
      addLog("already injecting");
      return;
    }
    if (!gmodPath) {
      addLog("no target selected");
      return;
    }
    setInjecting(true);
    setLaunchState("injecting");
    try {
      await invoke("inject", { gmodPath });
    } catch (error) {
      addLog(`injection failed: ${errMsg(error)}`);
      setLaunchState("fail");
    } finally {
      setInjecting(false);
      setTimeout(() => setLaunchState("idle"), 5000);
    }
  }, [injecting, gmodPath, addLog]);

  const notInstalled = version === "not installed";

  const handleInstall = useCallback(async () => {
    setInstallingUpdate(true);
    setInstallError(null);
    try {
      await handleDownload((message) => setInstallError(message));
    } finally {
      setInstallingUpdate(false);
    }
  }, [handleDownload]);

  return (
    <TooltipProvider>
      <div
        data-tauri-drag-region="deep"
        className="h-dvh overflow-hidden rounded-[12px] border border-[#222] bg-background"
      >
        <main className="grid h-full grid-rows-[auto_1fr_auto]">
          <header data-tauri-drag-region="deep" className="px-6 pt-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <img
                  src="/lje-logo-text.png"
                  alt="LJE"
                  draggable={false}
                  className="h-8 select-none"
                />
                <span
                  className={cn(
                    "size-2 rounded-full",
                    LED_CLASSES[launchState],
                  )}
                  aria-hidden="true"
                />
              </div>
              <div className="flex items-center gap-1">
                <span className="mr-2 font-mono text-[11px] text-muted-foreground">
                  {version}
                </span>
                <button
                  type="button"
                  onClick={() => setScriptsOpen(true)}
                  aria-label="Scripts"
                  className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                >
                  <Terminal className="size-4" />
                </button>
                <button
                  type="button"
                  onClick={() => setSettingsOpen(true)}
                  aria-label="Settings"
                  className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                >
                  <Settings className="size-4" />
                </button>
                <button
                  type="button"
                  onClick={() => void getCurrentWindow().minimize()}
                  aria-label="Minimize"
                  className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                >
                  <Minus className="size-4" />
                </button>
                <button
                  type="button"
                  onClick={() => void getCurrentWindow().close()}
                  aria-label="Close"
                  className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                >
                  <X className="size-4" />
                </button>
              </div>
            </div>
            {updateAvailable && !notInstalled && (
              <div className="mt-1.5 flex items-center text-[11px] text-[#cc5555]">
                <span>out of date:</span>
                <button
                  type="button"
                  onClick={() => void handleDownload()}
                  className="ml-1 cursor-pointer underline underline-offset-2"
                >
                  update
                </button>
              </div>
            )}
          </header>

          {notInstalled ? (
            <section className="flex flex-col items-center justify-center gap-2 px-6 text-center">
              <h1 className="text-lg font-semibold">Install LJE</h1>
              <p className="w-max text-sm text-muted-foreground">
                LJE is not installed, click the button below to install it.
              </p>
              <Button
                onClick={() => void handleInstall()}
                disabled={installingUpdate}
                className="mt-3 w-full max-w-xs"
              >
                {installingUpdate ? "Installing…" : "Install"}
              </Button>
              {installError && (
                <p className="mt-1 text-xs text-destructive">{installError}</p>
              )}
            </section>
          ) : (
            <>
              <section className="flex min-h-0 flex-col gap-3.5 px-6 pt-3">
                <div>
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="text-[10px] font-medium text-muted-foreground uppercase">
                      GMOD PATH
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={handleChangePath}
                    className="flex w-full cursor-pointer items-center justify-between rounded-[10px] bg-[#252525] px-3.5 py-2.5 text-left font-mono text-xs text-muted-foreground"
                  >
                    <span className="min-w-0 truncate">
                      {gmodPath ?? "finding GMod..."}
                    </span>
                    <Folder className="ml-1 inline size-3 shrink-0 text-muted-foreground" />
                  </button>
                </div>

                <div className="flex min-h-0 flex-1 flex-col">
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="text-[10px] font-medium text-muted-foreground uppercase">
                      LOG
                    </span>
                  </div>
                  <div className="flex min-h-0 flex-1 flex-col rounded-[10px] bg-[#252525] px-3.5 py-2.5">
                    <ScrollArea ref={scrollAreaRef} className="min-h-0 flex-1">
                      <div>
                        {logs.map((entry, index) => (
                          <div
                            key={index}
                            className="py-px font-mono text-xs break-words whitespace-pre-wrap"
                          >
                            <span className="text-[#505050]">
                              {entry.time}{" "}
                            </span>
                            <span
                              className={
                                entry.success
                                  ? "text-[#639922]"
                                  : "text-muted-foreground"
                              }
                            >
                              {entry.message}
                            </span>
                          </div>
                        ))}
                      </div>
                    </ScrollArea>
                  </div>
                </div>
              </section>

              <footer className="flex px-6 py-4">
                <Button
                  className="w-full cursor-pointer"
                  disabled={injecting}
                  onClick={handleInject}
                >
                  Launch
                </Button>
              </footer>
            </>
          )}
        </main>
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
        <ScriptsDialog open={scriptsOpen} onOpenChange={setScriptsOpen} />
      </div>
    </TooltipProvider>
  );
}

export default App;
