import { useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ChevronRight,
  FileText,
  FolderOpen,
  ImagePlus,
  Library,
  Mic2,
  MoreHorizontal,
  Pause,
  Play,
  Plus,
  RotateCcw,
  Search,
  Settings2,
  SkipBack,
  SkipForward,
  Sparkles,
  Square,
  Trash2,
  Upload,
  VolumeX,
} from "lucide-react";
import { useProjectStore } from "./store/project";
import type { ProjectSnapshot } from "./types";
import "./App.css";

const mediaExtensions = [
  "jpg",
  "jpeg",
  "png",
  "webp",
  "gif",
  "mp4",
  "mov",
  "m4v",
  "webm",
  "wav",
  "mp3",
  "m4a",
  "aac",
  "flac",
  "ogg",
  "opus",
];

function App() {
  const store = useProjectStore();
  const { project, activeBlockId, busy, error } = store;
  const [mode, setMode] = useState<"prepare" | "record">("prepare");
  if (!project)
    return (
      <ProjectHome
        onOpen={store.setProject}
        busy={busy}
        setBusy={store.setBusy}
        setError={store.setError}
        error={error}
      />
    );
  const activeBlock =
    project.blocks.find((block) => block.id === activeBlockId) ??
    project.blocks[0];
  return (
    <main className="app-shell">
      <header className="topbar">
        <button
          className="icon-button"
          onClick={store.reset}
          aria-label="Back to projects"
        >
          <ArrowLeft size={18} />
        </button>
        <div className="brand-mark">C</div>
        <div className="project-title">
          <strong>{project.name}</strong>
          <span>
            {project.aspectRatio} · {project.platformTarget}
          </span>
        </div>
        <div className="topbar-spacer" />
        <span className="save-state">
          <span /> Saved locally
        </span>
        <button className="icon-button" aria-label="Project settings">
          <Settings2 size={18} />
        </button>
        <button className="export-button">
          <Play size={15} fill="currentColor" /> Preview
        </button>
      </header>
      <section className="workspace">
        <aside className="script-panel">
          <PanelHeading eyebrow="Narration" title="Script blocks" />
          <div className="block-list">
            {project.blocks.map((block, index) => (
              <button
                key={block.id}
                className={`block-card ${block.id === activeBlock?.id ? "active" : ""}`}
                onClick={() => store.setActiveBlock(block.id)}
              >
                <span className="block-number">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span className="block-copy">{block.text}</span>
                <span className={`status-dot ${block.status}`} />
              </button>
            ))}
          </div>
          <ScriptImporter
            project={project}
            onUpdate={store.setProject}
            setBusy={store.setBusy}
            setError={store.setError}
          />
        </aside>
        <section className="stage-panel">
          {mode === "record" && activeBlock ? (
            <RecordingStudio
              project={project}
              block={activeBlock}
              onFinish={(updated) => {
                store.setProject(updated);
                setMode("prepare");
              }}
              onCancel={() => setMode("prepare")}
              setError={store.setError}
            />
          ) : (
            <>
              <div className="stage-toolbar">
                <div className="stage-tabs">
                  <button className="active">Prepare</button>
                  <button disabled>Record</button>
                  <button disabled>Review</button>
                </div>
                <span className="stage-hint">
                  Block {activeBlock ? activeBlock.position + 1 : 0} of{" "}
                  {project.blocks.length}
                </span>
              </div>
              <div
                className={`canvas-wrap ${project.aspectRatio === "9:16" ? "vertical" : "landscape"}`}
              >
                <div className="presentation-canvas">
                  {activeBlock?.tray[0] ? (
                    <AssetPreview
                      project={project}
                      assetId={activeBlock.tray[0].assetId}
                    />
                  ) : (
                    <div className="canvas-empty">
                      <div className="empty-orbit">
                        <ImagePlus size={25} />
                      </div>
                      <strong>Build this block’s visual sequence</strong>
                      <span>
                        Add media from the project dock, then arrange it in
                        presentation order.
                      </span>
                    </div>
                  )}
                  <div className="safe-zone" />
                </div>
              </div>
              {activeBlock && (
                <Tray
                  project={project}
                  blockId={activeBlock.id}
                  onUpdate={store.setProject}
                  setBusy={store.setBusy}
                  setError={store.setError}
                />
              )}
              <div className="record-ready">
                <div className="record-copy">
                  <span className="record-icon">
                    <Mic2 size={18} />
                  </span>
                  <div>
                    <strong>Prepare before recording</strong>
                    <span>Every block needs at least one visual cue.</span>
                  </div>
                </div>
                <button
                  className="record-button"
                  disabled={!activeBlock?.tray.length}
                  onClick={() => setMode("record")}
                >
                  <span /> Start recording
                </button>
              </div>
            </>
          )}
        </section>
        <MediaDock
          project={project}
          onUpdate={store.setProject}
          setBusy={store.setBusy}
          setError={store.setError}
        />
      </section>
      {busy && <div className="busy-bar" />}
      {error && (
        <ErrorToast error={error} dismiss={() => store.setError(null)} />
      )}
    </main>
  );
}

interface RecordingStatus {
  takeId: string;
  blockId: string;
  elapsedUs: number;
  paused: boolean;
}

function RecordingStudio({
  project,
  block,
  onFinish,
  onCancel,
  setError,
}: {
  project: ProjectSnapshot;
  block: ProjectSnapshot["blocks"][number];
  onFinish: (project: ProjectSnapshot) => void;
  onCancel: () => void;
  setError: (error: string | null) => void;
}) {
  const [phase, setPhase] = useState<
    "ready" | "countdown" | "recording" | "paused"
  >("ready");
  const [countdown, setCountdown] = useState(3);
  const [cueIndex, setCueIndex] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [startedAt, setStartedAt] = useState(0);
  const activeCue = block.tray[cueIndex];

  useEffect(() => {
    if (phase !== "recording") return;
    const timer = window.setInterval(
      () => setElapsed(performance.now() - startedAt),
      100,
    );
    return () => window.clearInterval(timer);
  }, [phase, startedAt]);

  useEffect(() => {
    function keydown(event: KeyboardEvent) {
      if (event.repeat) return;
      if (event.code === "ArrowRight") {
        event.preventDefault();
        void activateCue(Math.min(cueIndex + 1, block.tray.length - 1));
      }
      if (event.code === "ArrowLeft") {
        event.preventDefault();
        void activateCue(Math.max(cueIndex - 1, 0));
      }
      if (
        event.code === "Space" &&
        (phase === "recording" || phase === "paused")
      ) {
        event.preventDefault();
        void togglePause();
      }
    }
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  });

  async function begin() {
    setPhase("countdown");
    for (let value = 3; value > 0; value -= 1) {
      setCountdown(value);
      await new Promise((resolve) => window.setTimeout(resolve, 750));
    }
    try {
      await invoke<RecordingStatus>("start_recording", {
        projectPath: project.path,
        blockId: block.id,
        deviceName: null,
      });
      await invoke("record_cue", {
        eventType: "activate",
        trayItemId: block.tray[0].id,
      });
      setElapsed(0);
      setStartedAt(performance.now());
      setPhase("recording");
    } catch (reason) {
      setError(String(reason));
      setPhase("ready");
    }
  }

  async function activateCue(index: number) {
    if (index === cueIndex || phase === "ready" || phase === "countdown")
      return;
    try {
      await invoke("record_cue", {
        eventType: "activate",
        trayItemId: block.tray[index].id,
      });
      setCueIndex(index);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function togglePause() {
    try {
      if (phase === "recording") {
        await invoke("pause_recording");
        setPhase("paused");
      } else if (phase === "paused") {
        await invoke("resume_recording");
        setStartedAt(performance.now() - elapsed);
        setPhase("recording");
      }
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function stop() {
    try {
      onFinish(await invoke<ProjectSnapshot>("stop_recording"));
    } catch (reason) {
      setError(String(reason));
    }
  }

  const time = `${String(Math.floor(elapsed / 60000)).padStart(2, "0")}:${String(Math.floor(elapsed / 1000) % 60).padStart(2, "0")}.${String(Math.floor(elapsed / 100) % 10)}`;
  return (
    <div className="recording-studio">
      <div className="recording-head">
        <button
          className="quiet-button"
          onClick={onCancel}
          disabled={phase !== "ready"}
        >
          <ArrowLeft size={15} /> Back
        </button>
        <span className={`live-state ${phase}`}>
          <i /> {phase === "ready" ? "Ready" : phase}
        </span>
        <span className="record-time">{time}</span>
      </div>
      <div className="recording-body">
        <section className="teleprompter">
          <span className="eyebrow">
            Teleprompter · Block {block.position + 1}
          </span>
          <p>{block.text}</p>
          <div className="prompter-progress">
            <span style={{ width: phase === "ready" ? "0%" : "18%" }} />
          </div>
        </section>
        <section
          className={`live-canvas ${project.aspectRatio === "9:16" ? "vertical" : ""}`}
        >
          {activeCue && (
            <AssetPreview project={project} assetId={activeCue.assetId} />
          )}
          {phase === "countdown" && (
            <div className="countdown">{countdown}</div>
          )}
          {phase === "paused" && (
            <div className="paused-cover">
              <Pause size={30} />
              <span>Recording paused</span>
            </div>
          )}
        </section>
      </div>
      <div className="live-tray">
        {block.tray.map((item, index) => (
          <button
            key={item.id}
            className={index === cueIndex ? "active" : ""}
            onClick={() => activateCue(index)}
          >
            <span>{index + 1}</span>
            <AssetPreview project={project} assetId={item.assetId} />
          </button>
        ))}
      </div>
      <div className="record-controls">
        <button
          className="control secondary"
          onClick={() => activateCue(Math.max(0, cueIndex - 1))}
          disabled={phase === "ready"}
        >
          <SkipBack size={18} />
          <span>Previous</span>
        </button>
        {phase === "ready" ? (
          <button className="control begin" onClick={begin}>
            <Mic2 size={20} />
            <span>Begin take</span>
          </button>
        ) : (
          <>
            <button
              className="control secondary"
              onClick={togglePause}
              disabled={phase === "countdown"}
            >
              {phase === "paused" ? <Play size={18} /> : <Pause size={18} />}
              <span>{phase === "paused" ? "Resume" : "Pause"}</span>
            </button>
            <button
              className="control media-break"
              onClick={togglePause}
              disabled={phase === "countdown"}
            >
              <VolumeX size={18} />
              <span>Media break</span>
            </button>
            <button
              className="control stop"
              onClick={stop}
              disabled={phase === "countdown"}
            >
              <Square size={16} fill="currentColor" />
              <span>Stop & save</span>
            </button>
          </>
        )}
        <button
          className="control secondary"
          onClick={() =>
            activateCue(Math.min(block.tray.length - 1, cueIndex + 1))
          }
          disabled={phase === "ready"}
        >
          <SkipForward size={18} />
          <span>Next</span>
        </button>
      </div>
      <div className="record-shortcuts">
        <span>
          <kbd>Space</kbd> Pause
        </span>
        <span>
          <kbd>←</kbd>
          <kbd>→</kbd> Change media
        </span>
        <span>
          <RotateCcw size={12} /> Retakes preserve this take
        </span>
      </div>
    </div>
  );
}

function ProjectHome({
  onOpen,
  busy,
  setBusy,
  setError,
  error,
}: {
  onOpen: (project: ProjectSnapshot) => void;
  busy: boolean;
  setBusy: (value: boolean) => void;
  setError: (value: string | null) => void;
  error: string | null;
}) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [aspect, setAspect] = useState<"9:16" | "16:9">("9:16");
  async function chooseAndCreate() {
    if (!name.trim()) return;
    const parentPath = await open({
      directory: true,
      multiple: false,
      title: "Choose where to save your Cheeza project",
    });
    if (!parentPath) return;
    setBusy(true);
    setError(null);
    try {
      onOpen(
        await invoke<ProjectSnapshot>("create_project", {
          input: {
            parentPath,
            name: name.trim(),
            aspectRatio: aspect,
            platformTarget: aspect === "9:16" ? "TikTok" : "YouTube",
          },
        }),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  async function openExisting() {
    const projectPath = await open({
      directory: true,
      multiple: false,
      title: "Open a Cheeza project",
    });
    if (!projectPath) return;
    setBusy(true);
    setError(null);
    try {
      onOpen(await invoke<ProjectSnapshot>("open_project", { projectPath }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="home-shell">
      <nav className="home-nav">
        <div className="wordmark">
          <span>C</span> Cheeza
        </div>
        <button className="quiet-button">
          <Settings2 size={17} /> Preferences
        </button>
      </nav>
      <section className="home-content">
        <div className="home-intro">
          <span className="eyebrow accent">Script-led video production</span>
          <h1>
            Move from narration
            <br />
            to finished video.
          </h1>
          <p>
            Record in focused blocks, present your visuals live, and let Cheeza
            keep every cue, caption, and transition synchronized.
          </p>
          <div className="home-actions">
            <button
              className="primary-button"
              onClick={() => setCreating(true)}
            >
              <Plus size={18} /> New project
            </button>
            <button className="secondary-button" onClick={openExisting}>
              <FolderOpen size={18} /> Open project
            </button>
          </div>
        </div>
        <WorkflowPreview />
      </section>
      <footer className="home-footer">
        <span>Local-first · Offline-ready</span>
        <span>Built for focused creators</span>
      </footer>
      {creating && (
        <div className="modal-backdrop" onMouseDown={() => setCreating(false)}>
          <div
            className="modal"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <span className="eyebrow">Create project</span>
            <h2>Start with the canvas.</h2>
            <label>
              Project name
              <input
                autoFocus
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="My next story"
              />
            </label>
            <fieldset>
              <legend>Aspect ratio</legend>
              <button
                type="button"
                className={aspect === "9:16" ? "selected" : ""}
                onClick={() => setAspect("9:16")}
              >
                <span className="ratio vertical" />
                <b>Vertical</b>
                <small>9:16 · TikTok, Reels, Shorts</small>
              </button>
              <button
                type="button"
                className={aspect === "16:9" ? "selected" : ""}
                onClick={() => setAspect("16:9")}
              >
                <span className="ratio landscape" />
                <b>Landscape</b>
                <small>16:9 · YouTube</small>
              </button>
            </fieldset>
            <div className="modal-actions">
              <button
                className="secondary-button"
                onClick={() => setCreating(false)}
              >
                Cancel
              </button>
              <button
                className="primary-button"
                disabled={!name.trim() || busy}
                onClick={chooseAndCreate}
              >
                Choose location <ChevronRight size={17} />
              </button>
            </div>
          </div>
        </div>
      )}
      {busy && <div className="busy-bar" />}
      {error && <ErrorToast error={error} dismiss={() => setError(null)} />}
    </main>
  );
}

function WorkflowPreview() {
  return (
    <div className="workflow-preview">
      <div className="preview-glow" />
      <div className="preview-window">
        <div className="preview-top">
          <span />
          <span />
          <span />
          <b>INTRO · RECORDING</b>
        </div>
        <div className="preview-body">
          <div className="preview-script">
            <small>TELEPROMPTER</small>
            <p>Every great story begins with a clear point of view.</p>
            <div className="voice-line">
              <span />
              <i />
              <i />
              <i />
              <i />
              <i />
            </div>
          </div>
          <div className="preview-media">
            <div className="preview-frame">
              <Sparkles size={24} />
            </div>
            <div className="preview-tray">
              <span />
              <span className="selected" />
              <span />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
function PanelHeading({ eyebrow, title }: { eyebrow: string; title: string }) {
  return (
    <div className="panel-heading">
      <div>
        <span className="eyebrow">{eyebrow}</span>
        <h2>{title}</h2>
      </div>
      <button className="icon-button compact">
        <MoreHorizontal size={17} />
      </button>
    </div>
  );
}
function ErrorToast({
  error,
  dismiss,
}: {
  error: string;
  dismiss: () => void;
}) {
  return (
    <div className="error-toast">
      <span>{error}</span>
      <button onClick={dismiss}>Dismiss</button>
    </div>
  );
}

function ScriptImporter({ project, onUpdate, setBusy, setError }: AsyncProps) {
  const [editing, setEditing] = useState(!project.blocks.length);
  const [script, setScript] = useState(project.script);
  async function save() {
    if (!script.trim()) return;
    setBusy(true);
    setError(null);
    try {
      onUpdate(
        await invoke<ProjectSnapshot>("save_script", {
          projectPath: project.path,
          script,
        }),
      );
      setEditing(false);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  if (!editing)
    return (
      <button className="import-script" onClick={() => setEditing(true)}>
        <FileText size={16} /> Edit source script
      </button>
    );
  return (
    <div className="script-editor">
      <textarea
        value={script}
        onChange={(event) => setScript(event.target.value)}
        placeholder={
          "Paste your script here.\n\nParagraphs become focused recording blocks."
        }
      />
      <button onClick={save} disabled={!script.trim()}>
        Create blocks
      </button>
    </div>
  );
}

function MediaDock({ project, onUpdate, setBusy, setError }: AsyncProps) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(
    () =>
      project.assets.filter((asset) =>
        asset.name.toLowerCase().includes(query.toLowerCase()),
      ),
    [project.assets, query],
  );
  async function importFiles() {
    const selected = await open({
      multiple: true,
      title: "Import media",
      filters: [{ name: "Supported media", extensions: mediaExtensions }],
    });
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      onUpdate(
        await invoke<ProjectSnapshot>("import_media", {
          projectPath: project.path,
          sourcePaths: Array.isArray(selected) ? selected : [selected],
        }),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <aside className="media-panel">
      <div className="panel-heading">
        <div>
          <span className="eyebrow">Project</span>
          <h2>Media dock</h2>
        </div>
        <button className="icon-button compact" onClick={importFiles}>
          <Upload size={16} />
        </button>
      </div>
      <label className="search-box">
        <Search size={15} />
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search media"
        />
      </label>
      <div className="media-grid">
        {filtered.map((asset) => (
          <div className="asset-card" key={asset.id} draggable>
            <AssetPreview project={project} assetId={asset.id} />
            <span title={asset.name}>{asset.name}</span>
            <small>{asset.mediaType}</small>
          </div>
        ))}
        {!project.assets.length && (
          <div className="dock-empty">
            <Library size={24} />
            <strong>No media yet</strong>
            <span>Images, GIFs, videos and audio live here.</span>
            <button onClick={importFiles}>Import media</button>
          </div>
        )}
      </div>
    </aside>
  );
}

function Tray({
  project,
  blockId,
  onUpdate,
  setBusy,
  setError,
}: AsyncProps & { blockId: string }) {
  const block = project.blocks.find((item) => item.id === blockId)!;
  async function mutate(command: string, payload: object) {
    setBusy(true);
    setError(null);
    try {
      onUpdate(
        await invoke<ProjectSnapshot>(command, {
          projectPath: project.path,
          ...payload,
        }),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="tray">
      <div className="tray-label">
        <strong>Presentation tray</strong>
        <span>
          {block.tray.length} cue{block.tray.length === 1 ? "" : "s"}
        </span>
      </div>
      <div className="tray-items">
        {block.tray.map((item, index) => (
          <div className="tray-card" key={item.id}>
            <span className="cue-number">{index + 1}</span>
            <AssetPreview project={project} assetId={item.assetId} />
            <button
              onClick={() =>
                mutate("remove_tray_item", { trayItemId: item.id })
              }
            >
              <Trash2 size={13} />
            </button>
          </div>
        ))}
        <div className="tray-add">
          <Plus size={16} />
          <select
            value=""
            onChange={(event) =>
              event.target.value &&
              mutate("add_tray_item", { blockId, assetId: event.target.value })
            }
          >
            <option value="">Add cue</option>
            {project.assets.map((asset) => (
              <option key={asset.id} value={asset.id}>
                {asset.name}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}

function AssetPreview({
  project,
  assetId,
}: {
  project: ProjectSnapshot;
  assetId: string;
}) {
  const asset = project.assets.find((item) => item.id === assetId);
  if (!asset) return null;
  const source = convertFileSrc(`${project.path}/${asset.relativePath}`);
  if (asset.mediaType === "video") return <video src={source} muted />;
  if (asset.mediaType === "audio")
    return (
      <div className="audio-preview">
        <Mic2 size={23} />
      </div>
    );
  return <img src={source} alt="" />;
}
interface AsyncProps {
  project: ProjectSnapshot;
  onUpdate: (project: ProjectSnapshot) => void;
  setBusy: (value: boolean) => void;
  setError: (value: string | null) => void;
}
export default App;
