import {
  IconColorPicker as ColorPicker,
  IconDownload as Download,
  IconEraser as Eraser,
  IconEye as Eye,
  IconEyeOff as EyeOff,
  IconFolderOpen as FolderOpen,
  IconGridDots as GridDots,
  IconLayersSelected as Layers3,
  IconLine as Line,
  IconLoader2 as Loader2,
  IconPalette as Palette,
  IconPhoto as FileImage,
  IconPhoto as Photo,
  IconPencil as Pencil,
  IconPlayerPauseFilled as Pause,
  IconPlayerPlayFilled as Play,
  IconPlayerSkipBack as SkipBack,
  IconPlayerSkipForward as SkipForward,
  IconPlayerStopFilled as Stop,
  IconPlus as Plus,
  IconPointer as Pointer,
  IconRefresh as RefreshCw,
  IconRotate2 as RotateCcw,
  IconRotateClockwise as RotateCw,
  IconSettings as Settings,
  IconTrash as Trash2,
  IconZoomIn as ZoomIn,
  IconZoomOut as ZoomOut,
} from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Terminal, useTerminal } from "@wterm/react";
import "@wterm/react/css";
import sampleCloudUrl from "../assets/codex-color.png";

type Tool = "pencil" | "line" | "erase" | "picker";
type EffectKind = "none" | "rotate";
type EditorZoom = "fit" | 2 | 4 | 8;
type TerminalFont =
  | "SF Mono"
  | "Menlo"
  | "JetBrains Mono"
  | "Cascadia Mono"
  | "Fira Code"
  | "IBM Plex Mono"
  | "Courier New"
  | "Custom";

type RenderOptions = {
  width: number;
  threshold: number;
  invert: boolean;
  alpha_cutoff: number;
  glyph_mode: "braille" | "solid";
  color_mode: "ansi" | "monochrome";
};

type LayerEffect = {
  kind: EffectKind;
  degrees_per_frame: number;
};

type Layer = {
  id: string;
  name: string;
  data_url: string;
  visible: boolean;
  opacity: number;
  effect: LayerEffect;
  frameEdits: Record<string, string>;
};

type ImportedLayer = Omit<Layer, "frameEdits">;

type ImportedProject = {
  width: number;
  height: number;
  layers: ImportedLayer[];
  suggested_effects: EffectSuggestion[];
};

type EffectSuggestion = {
  name: string;
  description: string;
};

type Project = {
  width: number;
  height: number;
  layers: Layer[];
  suggestedEffects: EffectSuggestion[];
};

type Point = {
  x: number;
  y: number;
};

type Viewport = {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
};

type WorkingEdit = {
  canvas: HTMLCanvasElement;
  start: Point;
  last: Point;
  tool: Tool;
};

type OutputSize = {
  columns: number;
  rows: number | null;
};

type RenderLayout = {
  columns: number;
  rows: number;
  sample_width: number;
  sample_height: number;
};

type TerminalSettings = {
  fontFamily: TerminalFont;
  customFont: string;
  fontSize: number;
  lineHeight: number;
  background: string;
};

const defaultRenderOptions: RenderOptions = {
  width: 24,
  threshold: 180,
  invert: false,
  alpha_cutoff: 16,
  glyph_mode: "braille",
  color_mode: "ansi",
};

const minOutputWidth = 10;
const maxOutputWidth = 160;
const outputWidthPresets = [10, 16, 24, 40, 80, 120, 160];
const editorZoomLevels: EditorZoom[] = ["fit", 2, 4, 8];
const terminalFontSizeMin = 8;
const terminalFontSizeMax = 28;
const terminalFonts: TerminalFont[] = [
  "SF Mono",
  "Menlo",
  "JetBrains Mono",
  "Cascadia Mono",
  "Fira Code",
  "IBM Plex Mono",
  "Courier New",
  "Custom",
];
const defaultTerminalSettings: TerminalSettings = {
  fontFamily: "SF Mono",
  customFont: "",
  fontSize: 11,
  lineHeight: 1.16,
  background: "#20231d",
};
const ansiPreviewStart = "\x1b[?25l\x1b[2J\x1b[H";
const ansiDrawFrame = "\x1b[H\x1b[J";
const ansiReturnHome = "\x1b[H";
const ansiPreviewEnd = "\x1b[?25h";
const previewRenderDelayMs = 320;

function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [renderOptions, setRenderOptions] = useState(defaultRenderOptions);
  const [selectedLayerId, setSelectedLayerId] = useState<string | null>(null);
  const [tool, setTool] = useState<Tool>("pencil");
  const [brushSize, setBrushSize] = useState(14);
  const [brushColor, setBrushColor] = useState("#ffffff");
  const [terminalSettings, setTerminalSettings] = useState(defaultTerminalSettings);
  const [fontSizeDraft, setFontSizeDraft] = useState(String(defaultTerminalSettings.fontSize));
  const [showTerminalSettings, setShowTerminalSettings] = useState(false);
  const [showCharacterGrid, setShowCharacterGrid] = useState(false);
  const [editorZoom, setEditorZoom] = useState<EditorZoom>("fit");
  const [frameOnly, setFrameOnly] = useState(true);
  const [frameCount, setFrameCount] = useState(36);
  const [frames, setFrames] = useState<string[]>([]);
  const [renderLayout, setRenderLayout] = useState<RenderLayout | null>(null);
  const [currentFrame, setCurrentFrame] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [isRendering, setIsRendering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const viewportRef = useRef<Viewport | null>(null);
  const workingEditRef = useRef<WorkingEdit | null>(null);
  const drawVersionRef = useRef(0);
  const didAutoLoadRef = useRef(false);
  const imageCacheRef = useRef(new Map<string, Promise<HTMLImageElement>>());
  const editorDrawRequestRef = useRef<number | null>(null);
  const pendingEditorDrawRef = useRef<{
    overrideCanvas?: HTMLCanvasElement;
    linePreview?: Point;
  } | null>(null);
  const layoutRequestRef = useRef(0);
  const previewRenderRequestRef = useRef(0);
  const terminalStartedRef = useRef(false);
  const terminalWrapRef = useRef<HTMLDivElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const { ref: terminalRef, write } = useTerminal();
  const [terminalReadyRevision, setTerminalReadyRevision] = useState(0);

  const selectedLayer = useMemo(
    () => project?.layers.find((layer) => layer.id === selectedLayerId) ?? null,
    [project, selectedLayerId],
  );

  const previewRenderOptions = useMemo(
    () => ({
      ...renderOptions,
      width: renderOptions.width,
    }),
    [renderOptions],
  );

  const previewRows = useMemo(() => {
    const frame = frames[currentFrame] ?? "";
    return Math.max(10, renderLayout?.rows ?? frame.split("\n").length);
  }, [currentFrame, frames, renderLayout]);
  const outputSize = useMemo(() => {
    if (renderLayout) {
      return { columns: renderLayout.columns, rows: renderLayout.rows };
    }

    return measureRenderedFrame(frames[currentFrame] ?? frames[0] ?? "", renderOptions.width);
  }, [currentFrame, frames, renderLayout, renderOptions.width]);
  const previewCols = renderOptions.width + 1;
  const zoomLabel = editorZoom === "fit" ? "Fit" : `${editorZoom * 100}%`;
  const terminalStyle = terminalPreviewStyle(terminalSettings);
  const terminalTypographyKey = terminalTypographySignature(terminalSettings);

  const updateLayer = useCallback((layerId: string, patch: Partial<Layer>) => {
    setProject((current) => {
      if (!current) {
        return current;
      }

      return {
        ...current,
        layers: current.layers.map((layer) =>
          layer.id === layerId ? { ...layer, ...patch } : layer,
        ),
      };
    });
  }, []);

  const importDataUrl = useCallback(
    async (dataUrl: string, name: string) => {
      setIsImporting(true);
      setError(null);
      try {
        const response = await invoke<ImportedProject>("import_image", {
          payload: { data_url: dataUrl, name },
          options: renderOptions,
        });
        const layers = response.layers.map((layer) => ({
          ...layer,
          frameEdits: {},
        }));
        setProject({
          width: response.width,
          height: response.height,
          layers,
          suggestedEffects: response.suggested_effects,
        });
        setSelectedLayerId(layers[0]?.id ?? null);
        setCurrentFrame(0);
        setPlaying(true);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setIsImporting(false);
      }
    },
    [renderOptions],
  );

  const loadSample = useCallback(async () => {
    const response = await fetch(sampleCloudUrl);
    const blob = await response.blob();
    await importDataUrl(await blobToDataUrl(blob), "codex-color.png");
  }, [importDataUrl]);

  const handleFile = useCallback(
    async (file: File) => {
      await importDataUrl(await blobToDataUrl(file), file.name);
    },
    [importDataUrl],
  );

  const drawEditor = useCallback(
    async (overrideCanvas?: HTMLCanvasElement, linePreview?: Point) => {
      const canvas = canvasRef.current;
      if (!canvas || !project) {
        return;
      }

      const version = ++drawVersionRef.current;
      const rect = canvas.getBoundingClientRect();
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(rect.width * ratio));
      canvas.height = Math.max(1, Math.round(rect.height * ratio));

      const context = canvas.getContext("2d");
      if (!context) {
        return;
      }

      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, rect.width, rect.height);
      context.imageSmoothingEnabled = true;
      paintEditorBackdrop(context, rect.width, rect.height);

      const fitScale =
        Math.min(
          (rect.width - 48) / project.width,
          (rect.height - 48) / project.height,
        ) || 1;
      const scale = editorZoom === "fit" ? fitScale : editorZoom;
      const viewport = {
        x: (rect.width - project.width * scale) / 2,
        y: (rect.height - project.height * scale) / 2,
        width: project.width * scale,
        height: project.height * scale,
        scale,
      };
      viewportRef.current = viewport;

      context.fillStyle = "rgba(0, 0, 0, 0.18)";
      context.fillRect(viewport.x - 1, viewport.y - 1, viewport.width + 2, viewport.height + 2);

      for (const layer of project.layers) {
        if (!layer.visible) {
          continue;
        }

        const source =
          layer.id === selectedLayerId && overrideCanvas
            ? overrideCanvas
            : await loadCachedImage(effectiveLayerDataUrl(layer, currentFrame), imageCacheRef.current);
        if (version !== drawVersionRef.current) {
          return;
        }

        drawLayerAsDots(context, source, viewport, layer.opacity);
      }

      if (linePreview && workingEditRef.current) {
        const edit = workingEditRef.current;
        context.save();
        context.beginPath();
        context.lineCap = "round";
        context.lineJoin = "round";
        context.lineWidth = Math.max(1, brushSize) * viewport.scale;
        context.strokeStyle = edit.tool === "erase" ? "rgba(220, 70, 55, 0.72)" : brushColor;
        context.moveTo(viewport.x + edit.start.x * viewport.scale, viewport.y + edit.start.y * viewport.scale);
        context.lineTo(viewport.x + linePreview.x * viewport.scale, viewport.y + linePreview.y * viewport.scale);
        context.stroke();
        context.restore();
      }

      if (showCharacterGrid && renderLayout) {
        paintCharacterGrid(context, viewport, renderLayout);
      }

      context.strokeStyle = selectedLayer ? "#43d6d3" : "rgba(255, 255, 255, 0.16)";
      context.lineWidth = selectedLayer ? 2 : 1;
      context.strokeRect(viewport.x - 0.5, viewport.y - 0.5, viewport.width + 1, viewport.height + 1);
    },
    [
      brushColor,
      brushSize,
      currentFrame,
      editorZoom,
      project,
      renderLayout,
      selectedLayer,
      selectedLayerId,
      showCharacterGrid,
    ],
  );

  const requestEditorDraw = useCallback(
    (overrideCanvas?: HTMLCanvasElement, linePreview?: Point) => {
      pendingEditorDrawRef.current = { overrideCanvas, linePreview };
      if (editorDrawRequestRef.current !== null) {
        return;
      }

      editorDrawRequestRef.current = window.requestAnimationFrame(() => {
        editorDrawRequestRef.current = null;
        const pending = pendingEditorDrawRef.current;
        pendingEditorDrawRef.current = null;
        void drawEditor(pending?.overrideCanvas, pending?.linePreview);
      });
    },
    [drawEditor],
  );

  useEffect(() => {
    void drawEditor();
  }, [drawEditor]);

  useEffect(() => {
    return () => {
      if (editorDrawRequestRef.current !== null) {
        window.cancelAnimationFrame(editorDrawRequestRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const redraw = () => void drawEditor();
    window.addEventListener("resize", redraw);
    return () => window.removeEventListener("resize", redraw);
  }, [drawEditor]);

  useEffect(() => {
    if (!project) {
      setRenderLayout(null);
      return;
    }

    const requestId = ++layoutRequestRef.current;
    void invoke<RenderLayout>("render_layout", {
      request: {
        width: project.width,
        height: project.height,
        options: renderOptions,
      },
    })
      .then((layout) => {
        if (requestId === layoutRequestRef.current) {
          setRenderLayout(layout);
        }
      })
      .catch((caught) => {
        if (requestId === layoutRequestRef.current) {
          setRenderLayout(null);
          setError(errorMessage(caught));
        }
      });
  }, [project, renderOptions]);

  useEffect(() => {
    if (!project) {
      return;
    }

    const timeout = window.setTimeout(async () => {
      const requestId = ++previewRenderRequestRef.current;
      setIsRendering(true);
      setError(null);
      try {
        const rendered = await invoke<string[]>("render_preview_frames", {
          request: {
            width: project.width,
            height: project.height,
            layers: project.layers.map((layer) => ({
              id: layer.id,
              data_url: layer.data_url,
              visible: layer.visible,
              opacity: layer.opacity,
              effect: layer.effect,
              frame_data_urls: layer.frameEdits,
            })),
            options: previewRenderOptions,
            frame_count: frameCount,
          },
        });
        if (requestId !== previewRenderRequestRef.current) {
          return;
        }
        setFrames(rendered);
        setCurrentFrame((frame) => Math.min(frame, Math.max(0, rendered.length - 1)));
      } catch (caught) {
        if (requestId !== previewRenderRequestRef.current) {
          return;
        }
        setError(errorMessage(caught));
      } finally {
        if (requestId === previewRenderRequestRef.current) {
          setIsRendering(false);
        }
      }
    }, previewRenderDelayMs);

    return () => window.clearTimeout(timeout);
  }, [frameCount, previewRenderOptions, project]);

  useEffect(() => {
    if (terminalReadyRevision === 0) {
      return;
    }

    if (!terminalStartedRef.current) {
      write(ansiPreviewStart);
      terminalStartedRef.current = true;
    }

    write(`${ansiDrawFrame}${toTerminalFrame(frames[currentFrame] ?? "")}${ansiReturnHome}`);
    window.requestAnimationFrame(() => resetPreviewScroll(terminalWrapRef.current));
    window.setTimeout(() => resetPreviewScroll(terminalWrapRef.current), 0);
  }, [currentFrame, frames, terminalReadyRevision, write]);

  useEffect(() => {
    if (!playing || frames.length < 2) {
      return;
    }

    const interval = window.setInterval(() => {
      setCurrentFrame((frame) => (frame + 1) % frames.length);
    }, 80);
    return () => window.clearInterval(interval);
  }, [frames.length, playing]);

  const pointerToImagePoint = useCallback((event: React.PointerEvent<HTMLCanvasElement>) => {
    const viewport = viewportRef.current;
    const canvas = canvasRef.current;
    if (!viewport || !canvas || !project) {
      return null;
    }

    const rect = canvas.getBoundingClientRect();
    const x = (event.clientX - rect.left - viewport.x) / viewport.scale;
    const y = (event.clientY - rect.top - viewport.y) / viewport.scale;
    if (x < 0 || y < 0 || x > project.width || y > project.height) {
      return null;
    }

    return {
      x: Math.max(0, Math.min(project.width, x)),
      y: Math.max(0, Math.min(project.height, y)),
    };
  }, [project]);

  const startEdit = useCallback(
    async (event: React.PointerEvent<HTMLCanvasElement>) => {
      if (!project || (tool !== "picker" && !selectedLayer)) {
        return;
      }

      const point = pointerToImagePoint(event);
      if (!point) {
        return;
      }

      if (tool === "picker") {
        const sampledColor = await sampleProjectColor(
          project,
          currentFrame,
          point,
          imageCacheRef.current,
        );
        if (sampledColor) {
          setBrushColor(sampledColor);
        }
        return;
      }

      const editingLayer = selectedLayer;
      if (!editingLayer) {
        return;
      }

      event.currentTarget.setPointerCapture(event.pointerId);
      const editCanvas = document.createElement("canvas");
      editCanvas.width = project.width;
      editCanvas.height = project.height;
      const context = editCanvas.getContext("2d");
      if (!context) {
        return;
      }

      const image = await loadCachedImage(effectiveLayerDataUrl(editingLayer, currentFrame), imageCacheRef.current);
      context.drawImage(image, 0, 0);
      workingEditRef.current = { canvas: editCanvas, start: point, last: point, tool };

      if (tool !== "line") {
        paintStroke(context, point, point, tool, brushColor, brushSize);
      }
      requestEditorDraw(editCanvas);
    },
    [
      brushColor,
      brushSize,
      currentFrame,
      pointerToImagePoint,
      project,
      requestEditorDraw,
      selectedLayer,
      tool,
    ],
  );

  const moveEdit = useCallback(
    (event: React.PointerEvent<HTMLCanvasElement>) => {
      const edit = workingEditRef.current;
      if (!edit) {
        return;
      }

      const point = pointerToImagePoint(event);
      if (!point) {
        return;
      }

      if (edit.tool === "line") {
        requestEditorDraw(edit.canvas, point);
        return;
      }

      const context = edit.canvas.getContext("2d");
      if (!context) {
        return;
      }
      paintStroke(context, edit.last, point, edit.tool, brushColor, brushSize);
      edit.last = point;
      requestEditorDraw(edit.canvas);
    },
    [brushColor, brushSize, pointerToImagePoint, requestEditorDraw],
  );

  const finishEdit = useCallback(
    async (event: React.PointerEvent<HTMLCanvasElement>) => {
      const edit = workingEditRef.current;
      if (!edit || !project || !selectedLayer) {
        return;
      }

      const point = pointerToImagePoint(event);
      const context = edit.canvas.getContext("2d");
      if (point && context && edit.tool === "line") {
        paintStroke(context, edit.start, point, edit.tool, brushColor, brushSize);
      }

      workingEditRef.current = null;
      const dataUrl = await canvasToDataUrl(edit.canvas);
      setProject((current) => {
        if (!current) {
          return current;
        }

        return {
          ...current,
          layers: current.layers.map((layer) => {
          if (layer.id !== selectedLayer.id) {
            return layer;
          }

          if (!frameOnly) {
            return { ...layer, data_url: dataUrl, frameEdits: {} };
          }

          return {
            ...layer,
            frameEdits: {
              ...layer.frameEdits,
              [currentFrame]: dataUrl,
            },
          };
        }),
        };
      });
      void drawEditor();
    },
    [
      brushColor,
      brushSize,
      currentFrame,
      drawEditor,
      frameOnly,
      pointerToImagePoint,
      project,
      selectedLayer,
    ],
  );

  const clearFrameEdit = useCallback(() => {
    if (!selectedLayer) {
      return;
    }

    setProject((current) => {
      if (!current) {
        return current;
      }

      return {
        ...current,
        layers: current.layers.map((layer) => {
          if (layer.id !== selectedLayer.id) {
            return layer;
          }

          const { [currentFrame]: _removed, ...remaining } = layer.frameEdits;
          return { ...layer, frameEdits: remaining };
        }),
      };
    });
  }, [currentFrame, selectedLayer]);

  const stepEditorZoom = useCallback((direction: -1 | 1) => {
    setEditorZoom((current) => {
      const index = editorZoomLevels.indexOf(current);
      const nextIndex = Math.max(0, Math.min(editorZoomLevels.length - 1, index + direction));
      return editorZoomLevels[nextIndex];
    });
  }, []);

  const updateOutputWidth = useCallback((width: number) => {
    const nextWidth = clampOutputWidth(width);
    setRenderOptions((current) => ({ ...current, width: nextWidth }));
    setFrames([]);
    setCurrentFrame(0);
  }, []);

  const updateTerminalSettings = useCallback((patch: Partial<TerminalSettings>) => {
    setTerminalSettings((current) => ({ ...current, ...patch }));
  }, []);

  useEffect(() => {
    setFontSizeDraft(String(terminalSettings.fontSize));
  }, [terminalSettings.fontSize]);

  const commitFontSizeDraft = useCallback(() => {
    const parsedFontSize = Number(fontSizeDraft);
    const nextFontSize =
      fontSizeDraft.trim() === "" || !Number.isFinite(parsedFontSize)
        ? terminalSettings.fontSize
        : clampTerminalFontSize(parsedFontSize);

    setFontSizeDraft(String(nextFontSize));
    if (nextFontSize !== terminalSettings.fontSize) {
      updateTerminalSettings({ fontSize: nextFontSize });
    }
  }, [fontSizeDraft, terminalSettings.fontSize, updateTerminalSettings]);

  const exportAnsi = useCallback(() => {
    if (!frames.length) {
      return;
    }

    const payload = `${ansiPreviewStart}${frames.map((frame) => `${ansiDrawFrame}${toTerminalFrame(frame)}${ansiReturnHome}`).join("")}${ansiPreviewEnd}`;
    const url = URL.createObjectURL(new Blob([payload], { type: "text/plain" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = "blumdot-preview.ansi";
    link.click();
    URL.revokeObjectURL(url);
  }, [frames]);

  useEffect(() => {
    if (didAutoLoadRef.current) {
      return;
    }

    didAutoLoadRef.current = true;
    void loadSample();
  }, [loadSample]);

  const thumbnailUrl = project?.layers[0]?.data_url;
  const currentFrameLabel = `${String(currentFrame + 1).padStart(2, "0")} / ${String(frameCount).padStart(2, "0")}`;

  return (
    <main
      className="app-shell"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        const file = event.dataTransfer.files.item(0);
        if (file) {
          void handleFile(file);
        }
      }}
    >
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">⠿</span>
          <span>Blumdot</span>
        </div>
        <div className="topbar-actions primary-actions">
          <button className="button" onClick={() => fileInputRef.current?.click()} disabled={isImporting}>
            <FolderOpen size={16} />
            Import
          </button>
          <button className="button ghost-button" onClick={loadSample} disabled={isImporting}>
            <FileImage size={16} />
            Sample
          </button>
        </div>
        <div className="topbar-actions transport-actions">
          <button className="button icon-button" onClick={() => setCurrentFrame(0)} aria-label="First frame">
            <SkipBack size={17} />
          </button>
          <button
            className="button play-button"
            onClick={() => setPlaying((value) => !value)}
            aria-label={playing ? "Pause" : "Play"}
            title={playing ? "Pause" : "Play"}
          >
            {playing ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" />}
          </button>
          <button className="button icon-button" onClick={() => setCurrentFrame((frame) => (frame + 1) % frameCount)} aria-label="Next frame">
            <SkipForward size={17} />
          </button>
          <div className="fps-control">
            <span>{Math.round(1000 / 80)}</span>
            FPS
          </div>
        </div>
        <div className="topbar-actions export-actions">
          <button className="button">
            <RotateCw size={16} />
            Effects
          </button>
          <button className="button" onClick={exportAnsi} disabled={!frames.length}>
            <Download size={16} />
            Export
          </button>
          <button className="button icon-button" aria-label="Settings">
            <Settings size={17} />
          </button>
          <input
            ref={fileInputRef}
            className="visually-hidden"
            type="file"
            accept="image/*,.svg"
            onChange={(event) => {
              const file = event.currentTarget.files?.item(0);
              if (file) {
                void handleFile(file);
              }
              event.currentTarget.value = "";
            }}
          />
        </div>
      </header>

      <section className="workspace">
        <aside className="left-rail">
          <div className="tool-group" aria-label="Tools">
            <ToolButton active={tool === "pencil"} label="Pencil" onClick={() => setTool("pencil")}>
              <Pencil size={20} />
            </ToolButton>
            <ToolButton active={tool === "line"} label="Line" onClick={() => setTool("line")}>
              <Line size={20} stroke={1.8} />
            </ToolButton>
            <ToolButton active={tool === "erase"} label="Erase" onClick={() => setTool("erase")}>
              <Eraser size={20} />
            </ToolButton>
            <ToolButton active={tool === "picker"} label="Pick color" onClick={() => setTool("picker")}>
              <ColorPicker size={20} />
            </ToolButton>
            <ToolButton active={false} label="Select" onClick={() => undefined}>
              <Pointer size={20} />
            </ToolButton>
          </div>
          <div className="color-stack">
            <label className="color-field" title="Brush color">
              <span>Color</span>
              <input
                type="color"
                value={brushColor}
                onChange={(event) => setBrushColor(event.currentTarget.value)}
                aria-label="Brush color"
              />
            </label>
            <label className="brush-size-control" title={`Brush size: ${brushSize}`}>
              <span className="brush-size-label">Size</span>
              <input
                type="range"
                min={2}
                max={48}
                value={brushSize}
                aria-label="Brush size"
                onChange={(event) => setBrushSize(Number(event.currentTarget.value))}
              />
              <span className="brush-size-value">{brushSize}</span>
            </label>
          </div>
        </aside>

        <aside className="right-stack">
          <section className="panel layers-panel">
            <div className="panel-title">
              <span><Layers3 size={17} /> Layers</span>
              <button className="small-icon" aria-label="Add layer">
                <Plus size={17} />
              </button>
            </div>
            <div className="layer-list">
              {project?.layers.map((layer) => (
                <div
                  key={layer.id}
                  role="button"
                  tabIndex={0}
                  className={`layer-row ${layer.id === selectedLayerId ? "active" : ""}`}
                  onClick={() => setSelectedLayerId(layer.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedLayerId(layer.id);
                    }
                  }}
                >
                  <span
                    className="layer-thumb"
                    style={{ backgroundImage: `url(${effectiveLayerDataUrl(layer, currentFrame)})` }}
                  />
                  <button
                    className="small-icon"
                    onClick={(event) => {
                      event.stopPropagation();
                      updateLayer(layer.id, { visible: !layer.visible });
                    }}
                    aria-label={layer.visible ? "Hide layer" : "Show layer"}
                  >
                    {layer.visible ? <Eye size={16} /> : <EyeOff size={16} />}
                  </button>
                  <input
                    value={layer.name}
                    onChange={(event) => updateLayer(layer.id, { name: event.currentTarget.value })}
                    onClick={(event) => event.stopPropagation()}
                  />
                  <span className="frame-badge">{Object.keys(layer.frameEdits).length || ""}</span>
                  {project.layers.length > 1 && (
                    <button
                      className="small-icon"
                      onClick={(event) => {
                        event.stopPropagation();
                        setProject({
                          ...project,
                          layers: project.layers.filter((candidate) => candidate.id !== layer.id),
                        });
                      }}
                      aria-label="Delete layer"
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                </div>
              ))}
            </div>
            {selectedLayer && (
              <div className="layer-controls">
                <label>
                  <span>Blend</span>
                  <select>
                    <option>Normal</option>
                  </select>
                </label>
                <label>
                  <span>Opacity</span>
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.05}
                    value={selectedLayer.opacity}
                    onChange={(event) =>
                      updateLayer(selectedLayer.id, { opacity: Number(event.currentTarget.value) })
                    }
                  />
                </label>
                <label>
                  <span>Effect</span>
                  <select
                    value={selectedLayer.effect.kind}
                    onChange={(event) =>
                      updateLayer(selectedLayer.id, {
                        effect: {
                          ...selectedLayer.effect,
                          kind: event.currentTarget.value as EffectKind,
                        },
                      })
                    }
                  >
                    <option value="none">None</option>
                    <option value="rotate">Rotate</option>
                  </select>
                </label>
                {selectedLayer.effect.kind === "rotate" && (
                  <label>
                    <span>Degrees</span>
                    <input
                      type="number"
                      value={selectedLayer.effect.degrees_per_frame}
                      onChange={(event) =>
                        updateLayer(selectedLayer.id, {
                          effect: {
                            ...selectedLayer.effect,
                            degrees_per_frame: Number(event.currentTarget.value),
                          },
                        })
                      }
                    />
                  </label>
                )}
                <label className="toggle-row">
                  <input
                    type="checkbox"
                    checked={frameOnly}
                    onChange={(event) => setFrameOnly(event.currentTarget.checked)}
                  />
                  <span>Frame only</span>
                </label>
                <button className="button secondary" onClick={clearFrameEdit}>
                  <Trash2 size={15} />
                  Clear Frame
                </button>
              </div>
            )}
          </section>
        </aside>

        <section className="editor-pane">
          <div className="canvas-toolbar">
            <div className="toolbar-section">
              <span>Rotate</span>
              <button className="small-icon" aria-label="Rotate counterclockwise">
                <RotateCcw size={15} />
              </button>
              <button className="small-icon" aria-label="Rotate clockwise">
                <RotateCw size={15} />
              </button>
              <button
                className={`button compact-button ${showCharacterGrid ? "active-toggle" : ""}`}
                onClick={() => setShowCharacterGrid((value) => !value)}
                aria-pressed={showCharacterGrid}
                aria-label="Toggle character grid"
                title="Toggle character grid"
              >
                <GridDots size={15} />
                Grid
              </button>
            </div>
            <div className="toolbar-section">
              <span>Width</span>
              <input
                className="width-input"
                type="number"
                min={minOutputWidth}
                max={maxOutputWidth}
                step={1}
                value={renderOptions.width}
                onChange={(event) => updateOutputWidth(Number(event.currentTarget.value))}
                aria-label="Output width"
              />
              <select
                className="width-preset-select"
                value=""
                onChange={(event) => {
                  const value = Number(event.currentTarget.value);
                  if (value) {
                    updateOutputWidth(value);
                  }
                }}
                aria-label="Width presets"
              >
                <option value="">Presets</option>
                {outputWidthPresets.map((width) => (
                  <option key={width} value={width}>
                    {width} cols
                  </option>
                ))}
              </select>
              <span className="output-size-label">{renderOutputSize(outputSize)}</span>
              <button
                className="small-icon"
                onClick={() => stepEditorZoom(-1)}
                aria-label="Zoom out"
                disabled={editorZoom === editorZoomLevels[0]}
              >
                <ZoomOut size={15} />
              </button>
              <span className="zoom-label">{zoomLabel}</span>
              <button
                className="small-icon"
                onClick={() => stepEditorZoom(1)}
                aria-label="Zoom in"
                disabled={editorZoom === editorZoomLevels[editorZoomLevels.length - 1]}
              >
                <ZoomIn size={15} />
              </button>
            </div>
          </div>
          <div className="canvas-frame">
            {!project && (
              <div className="drop-state">
                <Photo size={38} />
                <span>Drop image</span>
              </div>
            )}
            <canvas
              ref={canvasRef}
              onPointerDown={(event) => void startEdit(event)}
              onPointerMove={moveEdit}
              onPointerUp={finishEdit}
              onPointerCancel={finishEdit}
            />
          </div>
        </section>

        <section className="preview-pane">
          <div className="preview-header">
            <span>Preview</span>
            {(isRendering || isImporting) && <Loader2 className="spin" size={16} />}
          </div>
          <div
            ref={terminalWrapRef}
            className={`terminal-wrap ${renderOptions.color_mode === "monochrome" ? "monochrome" : ""}`}
            style={terminalStyle}
          >
            {renderOptions.color_mode === "ansi" && (
              <div className="terminal-titlebar">
                <span className="traffic red" />
                <span className="traffic yellow" />
                <span className="traffic green" />
                <span>blumdot preview</span>
              </div>
            )}
            <Terminal
              key={terminalTypographyKey}
              ref={terminalRef}
              className="preview-terminal"
              cols={previewCols}
              rows={previewRows}
              theme="monokai"
              style={{ ...terminalStyle, height: "100%" }}
              onReady={() => {
                terminalStartedRef.current = false;
                setTerminalReadyRevision((revision) => revision + 1);
              }}
              onData={() => undefined}
            />
          </div>
          <div className="preview-controls">
            <button
              className="button icon-button"
              onClick={() => setPlaying((value) => !value)}
              aria-label={playing ? "Pause preview" : "Play preview"}
              title={playing ? "Pause preview" : "Play preview"}
            >
              {playing ? <Pause size={17} fill="currentColor" /> : <Play size={17} fill="currentColor" />}
            </button>
            <button className="button icon-button" onClick={() => setPlaying(false)} aria-label="Stop preview">
              <Stop size={15} />
            </button>
            <label className="button icon-button preview-bg-button" title="Preview background">
              <Palette size={16} />
              <span
                className="preview-bg-swatch"
                style={{ backgroundColor: terminalSettings.background }}
                aria-hidden="true"
              />
              <input
                type="color"
                value={terminalSettings.background}
                onChange={(event) => updateTerminalSettings({ background: event.currentTarget.value })}
                aria-label="Preview background"
              />
            </label>
            <button
              className={`button icon-button ${showTerminalSettings ? "active-toggle" : ""}`}
              onClick={() => setShowTerminalSettings((value) => !value)}
              aria-pressed={showTerminalSettings}
              aria-label="Terminal settings"
              title="Terminal settings"
            >
              <Settings size={16} />
            </button>
            <div className="fps-select">{Math.round(1000 / 80)} FPS</div>
          </div>
          {showTerminalSettings && (
            <div className="terminal-settings-panel">
              <div className="terminal-settings-header">
                <div>
                  <span>Terminal</span>
                  <strong>Appearance</strong>
                </div>
                <button
                  className="button secondary reset-terminal-button"
                  onClick={() => setTerminalSettings(defaultTerminalSettings)}
                  aria-label="Reset terminal appearance"
                  title="Reset terminal appearance"
                >
                  <RefreshCw size={14} />
                  Reset
                </button>
              </div>
              <div className="terminal-settings-grid">
                <div className="terminal-field terminal-mode-field">
                  <span>Render</span>
                  <div className="segmented-control" aria-label="Render mode">
                    <button
                      type="button"
                      className={renderOptions.color_mode === "ansi" ? "active" : ""}
                      onClick={() => setRenderOptions((current) => ({ ...current, color_mode: "ansi" }))}
                      aria-pressed={renderOptions.color_mode === "ansi"}
                    >
                      Color
                    </button>
                    <button
                      type="button"
                      className={renderOptions.color_mode === "monochrome" ? "active" : ""}
                      onClick={() => setRenderOptions((current) => ({ ...current, color_mode: "monochrome" }))}
                      aria-pressed={renderOptions.color_mode === "monochrome"}
                    >
                      Mono
                    </button>
                  </div>
                </div>
                <label className="terminal-field terminal-font-field">
                  <span>Font</span>
                  <select
                    value={terminalSettings.fontFamily}
                    onChange={(event) =>
                      updateTerminalSettings({ fontFamily: event.currentTarget.value as TerminalFont })
                    }
                  >
                    {terminalFonts.map((font) => (
                      <option key={font} value={font}>
                        {font}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="terminal-field">
                  <span>Size</span>
                  <input
                    type="number"
                    min={terminalFontSizeMin}
                    max={terminalFontSizeMax}
                    step={1}
                    value={fontSizeDraft}
                    onChange={(event) => {
                      const nextDraft = event.currentTarget.value;
                      const nextFontSize = Number(nextDraft);

                      setFontSizeDraft(nextDraft);
                      if (
                        nextDraft.trim() !== "" &&
                        Number.isInteger(nextFontSize) &&
                        nextFontSize >= terminalFontSizeMin &&
                        nextFontSize <= terminalFontSizeMax
                      ) {
                        updateTerminalSettings({ fontSize: nextFontSize });
                      }
                    }}
                    onBlur={commitFontSizeDraft}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.currentTarget.blur();
                      }
                    }}
                  />
                </label>
                {terminalSettings.fontFamily === "Custom" && (
                  <label className="terminal-field terminal-custom-font-field">
                    <span>Custom font</span>
                    <input
                      type="text"
                      value={terminalSettings.customFont}
                      placeholder="Font family"
                      onChange={(event) => updateTerminalSettings({ customFont: event.currentTarget.value })}
                    />
                  </label>
                )}
                <label className="terminal-field terminal-range-field">
                  <span>
                    Line height
                    <strong>{terminalSettings.lineHeight.toFixed(2)}</strong>
                  </span>
                  <input
                    type="range"
                    min={0.9}
                    max={1.8}
                    step={0.05}
                    value={terminalSettings.lineHeight}
                    onChange={(event) =>
                      updateTerminalSettings({
                        lineHeight: clampNumber(Number(event.currentTarget.value), 0.9, 1.8),
                      })
                    }
                  />
                </label>
                <label className="terminal-field terminal-color-field">
                  <span>Background</span>
                  <div className="terminal-color-row">
                    <input
                      type="color"
                      value={terminalSettings.background}
                      onChange={(event) => updateTerminalSettings({ background: event.currentTarget.value })}
                      aria-label="Terminal background color"
                    />
                    <code>{terminalSettings.background}</code>
                  </div>
                </label>
              </div>
            </div>
          )}
          {error && <div className="error-line">{error}</div>}
        </section>

        <div className="timeline">
          <div className="timeline-meta">
            <span>Frames</span>
            <button className="small-icon" aria-label="Add frame">
              <Plus size={17} />
            </button>
            <input
              type="number"
              min={1}
              max={120}
              value={frameCount}
              onChange={(event) => setFrameCount(Number(event.currentTarget.value))}
            />
          </div>
          <div className="frame-strip">
            {Array.from({ length: frameCount }, (_, index) => (
              <button
                key={index}
                className={`frame-chip ${index === currentFrame ? "active" : ""}`}
                onClick={() => setCurrentFrame(index)}
                style={thumbnailUrl ? { backgroundImage: `url(${thumbnailUrl})` } : undefined}
              >
                <span>{index + 1}</span>
              </button>
            ))}
          </div>
          <div className="ruler">
            {Array.from({ length: 7 }, (_, index) => (
              <span key={index}>{index === 0 ? 1 : index * 5}</span>
            ))}
          </div>
        </div>

        <footer className="statusbar">
          <span>{renderOutputSize(outputSize)}</span>
          <span>{Math.round(1000 / 80)} FPS</span>
          <span>Frame {currentFrameLabel}</span>
          <span>Layer: {selectedLayer?.name ?? "None"}</span>
          <span className="status-zoom">Zoom: {zoomLabel}</span>
        </footer>
      </section>
    </main>
  );
}

function ToolButton({
  active,
  children,
  label,
  onClick,
}: {
  active: boolean;
  children: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={`tool-button ${active ? "active" : ""}`}
      onClick={onClick}
      aria-label={label}
      title={label}
    >
      {children}
    </button>
  );
}

function effectiveLayerDataUrl(layer: Layer, frame: number) {
  return layer.frameEdits[frame] ?? layer.data_url;
}

function toTerminalFrame(frame: string) {
  return frame.replace(/\n/g, "\r\n");
}

function measureRenderedFrame(frame: string, columns: number): OutputSize {
  if (!frame) {
    return { columns, rows: null };
  }

  const lines = frame.split("\n");
  return {
    columns,
    rows: Math.max(1, lines.length),
  };
}

function renderOutputSize(size: OutputSize) {
  return `${size.columns} cols × ${size.rows ?? "–"} rows`;
}

function clampOutputWidth(width: number) {
  return Math.round(clampNumber(width, minOutputWidth, maxOutputWidth));
}

function clampTerminalFontSize(fontSize: number) {
  return Math.round(clampNumber(fontSize, terminalFontSizeMin, terminalFontSizeMax));
}

function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) {
    return min;
  }

  return Math.max(min, Math.min(max, value));
}

function terminalPreviewStyle(settings: TerminalSettings) {
  const fontSize = clampTerminalFontSize(settings.fontSize);
  const lineHeight = clampNumber(settings.lineHeight, 0.9, 1.8);
  const rowHeight = Math.max(1, Math.round(fontSize * lineHeight));

  return {
    "--preview-bg": settings.background,
    "--preview-font-family": terminalFontFamilyValue(settings),
    "--preview-font-size": `${fontSize}px`,
    "--preview-line-height": String(lineHeight),
    "--preview-row-height": `${rowHeight}px`,
    "--term-bg": settings.background,
    "--term-color-0": settings.background,
    "--term-font-family": terminalFontFamilyValue(settings),
    "--term-font-size": `${fontSize}px`,
    "--term-line-height": String(lineHeight),
    "--term-row-height": `${rowHeight}px`,
  } as React.CSSProperties;
}

function terminalTypographySignature(settings: TerminalSettings) {
  return [
    terminalFontFamilyValue(settings),
    clampTerminalFontSize(settings.fontSize),
    clampNumber(settings.lineHeight, 0.9, 1.8),
  ].join("|");
}

function terminalFontFamilyValue(settings: TerminalSettings) {
  const font =
    settings.fontFamily === "Custom"
      ? settings.customFont.trim() || defaultTerminalSettings.fontFamily
      : settings.fontFamily;

  return `${quoteFontFamily(font)}, "SF Mono", Menlo, Consolas, monospace`;
}

function quoteFontFamily(font: string) {
  return /^[a-z0-9-]+$/i.test(font) ? font : `"${font.replace(/"/g, '\\"')}"`;
}

function resetPreviewScroll(root: HTMLDivElement | null) {
  if (!root) {
    return;
  }

  root.scrollLeft = 0;
  root.scrollTop = 0;
  root.querySelectorAll<HTMLElement>("*").forEach((element) => {
    element.scrollLeft = 0;
    element.scrollTop = 0;
  });
}

function paintEditorBackdrop(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
) {
  context.fillStyle = "#101514";
  context.fillRect(0, 0, width, height);

  context.save();
  context.globalAlpha = 0.5;
  context.fillStyle = "#596160";
  for (let y = 10; y < height; y += 16) {
    for (let x = 10; x < width; x += 16) {
      context.beginPath();
      context.arc(x, y, 1.6, 0, Math.PI * 2);
      context.fill();
    }
  }
  context.restore();
}

function paintCharacterGrid(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  layout: RenderLayout,
) {
  const columns = Math.max(1, layout.columns);
  const rows = Math.max(1, layout.rows);
  const cellWidth = viewport.width / columns;
  const cellHeight = viewport.height / rows;
  const majorEvery = columns >= 80 ? 10 : 5;

  context.save();
  context.beginPath();
  context.rect(viewport.x, viewport.y, viewport.width, viewport.height);
  context.clip();

  context.lineWidth = 1;
  for (let column = 1; column < columns; column += 1) {
    const isMajor = column % majorEvery === 0;
    context.strokeStyle = isMajor ? "rgba(67, 214, 211, 0.5)" : "rgba(238, 242, 239, 0.18)";
    context.beginPath();
    const x = viewport.x + column * cellWidth;
    context.moveTo(x, viewport.y);
    context.lineTo(x, viewport.y + viewport.height);
    context.stroke();
  }

  for (let row = 1; row < rows; row += 1) {
    const isMajor = row % majorEvery === 0;
    context.strokeStyle = isMajor ? "rgba(67, 214, 211, 0.5)" : "rgba(238, 242, 239, 0.18)";
    context.beginPath();
    const y = viewport.y + row * cellHeight;
    context.moveTo(viewport.x, y);
    context.lineTo(viewport.x + viewport.width, y);
    context.stroke();
  }

  context.restore();
}

function drawLayerAsDots(
  context: CanvasRenderingContext2D,
  source: CanvasImageSource,
  viewport: Viewport,
  opacity: number,
) {
  const spacing = 5;
  const sampleWidth = Math.max(1, Math.ceil(viewport.width / spacing));
  const sampleHeight = Math.max(1, Math.ceil(viewport.height / spacing));
  const sampleCanvas = document.createElement("canvas");
  sampleCanvas.width = sampleWidth;
  sampleCanvas.height = sampleHeight;

  const sampleContext = sampleCanvas.getContext("2d", {
    willReadFrequently: true,
  });
  if (!sampleContext) {
    return;
  }

  sampleContext.clearRect(0, 0, sampleWidth, sampleHeight);
  sampleContext.drawImage(source, 0, 0, sampleWidth, sampleHeight);
  const pixels = sampleContext.getImageData(0, 0, sampleWidth, sampleHeight).data;

  context.save();
  context.globalAlpha = opacity;
  for (let y = 0; y < sampleHeight; y += 1) {
    for (let x = 0; x < sampleWidth; x += 1) {
      const offset = (y * sampleWidth + x) * 4;
      const alpha = pixels[offset + 3];
      if (alpha < 10) {
        continue;
      }

      context.fillStyle = `rgba(${pixels[offset]}, ${pixels[offset + 1]}, ${pixels[offset + 2]}, ${alpha / 255})`;
      context.beginPath();
      context.arc(
        viewport.x + x * spacing + spacing / 2,
        viewport.y + y * spacing + spacing / 2,
        Math.max(1.2, spacing * 0.38),
        0,
        Math.PI * 2,
      );
      context.fill();
    }
  }
  context.restore();
}

function paintStroke(
  context: CanvasRenderingContext2D,
  from: Point,
  to: Point,
  tool: Tool,
  color: string,
  size: number,
) {
  context.save();
  context.beginPath();
  context.lineCap = "round";
  context.lineJoin = "round";
  context.lineWidth = size;
  context.globalCompositeOperation = tool === "erase" ? "destination-out" : "source-over";
  context.strokeStyle = color;
  context.moveTo(from.x, from.y);
  context.lineTo(to.x, to.y);
  context.stroke();
  if (from.x === to.x && from.y === to.y) {
    context.arc(from.x, from.y, size / 2, 0, Math.PI * 2);
    context.fillStyle = color;
    context.fill();
  }
  context.restore();
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("failed to load image layer"));
    image.src = src;
  });
}

function loadCachedImage(
  src: string,
  cache: Map<string, Promise<HTMLImageElement>>,
): Promise<HTMLImageElement> {
  const cached = cache.get(src);
  if (cached) {
    return cached;
  }

  const image = loadImage(src).catch((error) => {
    cache.delete(src);
    throw error;
  });
  cache.set(src, image);
  return image;
}

async function sampleProjectColor(
  project: Project,
  frame: number,
  point: Point,
  cache: Map<string, Promise<HTMLImageElement>>,
) {
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return null;
  }

  const sampleX = Math.max(0, Math.min(project.width - 1, Math.floor(point.x)));
  const sampleY = Math.max(0, Math.min(project.height - 1, Math.floor(point.y)));

  for (const layer of [...project.layers].reverse()) {
    if (!layer.visible || layer.opacity <= 0) {
      continue;
    }

    const image = await loadCachedImage(effectiveLayerDataUrl(layer, frame), cache);
    context.clearRect(0, 0, 1, 1);
    context.drawImage(image, sampleX, sampleY, 1, 1, 0, 0, 1, 1);
    const [red, green, blue, alpha] = context.getImageData(0, 0, 1, 1).data;
    if (alpha * layer.opacity > 8) {
      return rgbToHex(red, green, blue);
    }
  }

  return null;
}

function rgbToHex(red: number, green: number, blue: number) {
  return `#${[red, green, blue]
    .map((channel) => channel.toString(16).padStart(2, "0"))
    .join("")}`;
}

function canvasToDataUrl(canvas: HTMLCanvasElement): Promise<string> {
  return new Promise((resolve) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        resolve(canvas.toDataURL("image/png"));
        return;
      }

      void blobToDataUrl(blob).then(resolve);
    }, "image/png");
  });
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("failed to read image"));
    reader.readAsDataURL(blob);
  });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export default App;
