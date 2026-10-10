/**
 * VECTORIZER STUDIO – Interactive Frontend Client
 */

(function () {
  'use strict';

  // State Management
  const state = {
    currentImageBlob: null,
    currentImageDimensions: { width: 512, height: 512 },
    zoom: 1.0,
    pan: { x: 0, y: 0 },
    isPanning: false,
    startPan: { x: 0, y: 0 },
    viewMode: 'split', // 'split', 'curtain', 'vector', 'raster'
    curtainPct: 50,
    isDraggingCurtain: false,
    activePreset: 'balanced',
    params: {
      colors: 16,
      lineTol: 0.80,
      arcTol: 0.70,
      curveTol: 0.80,
      corner: 135,
      snap: 2.0,
      stacked: false
    },
    demos: [],
    currentDemoId: 'circle_ring',
    latestSvg: '',
    latestOverlaySvg: '',
    latestDxf: '',
    latestStats: null,
    autoVectorizeTimer: null
  };

  // DOM Elements
  const DOM = {
    canvasViewport: document.getElementById('canvasViewport'),
    canvasStage: document.getElementById('canvasStage'),
    viewContainer: document.getElementById('viewContainer'),
    frameRaster: document.getElementById('frameRaster'),
    frameVector: document.getElementById('frameVector'),
    rasterImg: document.getElementById('rasterImg'),
    vectorSvgHost: document.getElementById('vectorSvgHost'),
    vectorOverlayHost: document.getElementById('vectorOverlayHost'),
    curtainDivider: document.getElementById('curtainDivider'),
    dropOverlay: document.getElementById('dropOverlay'),
    fileInput: document.getElementById('fileInput'),
    sceneButtons: document.getElementById('sceneButtons'),
    exportDropdown: document.getElementById('exportDropdown'),
    btnExportMenu: document.getElementById('btnExportMenu'),
    btnUpload: document.getElementById('btnUpload'),
    btnVectorize: document.getElementById('btnVectorize'),
    viewModeGroup: document.getElementById('viewModeGroup'),
    btnZoomIn: document.getElementById('btnZoomIn'),
    btnZoomOut: document.getElementById('btnZoomOut'),
    btnResetView: document.getElementById('btnResetView'),
    btnFitView: document.getElementById('btnFitView'),
    zoomLevelText: document.getElementById('zoomLevelText'),
    statLatencyBadge: document.getElementById('statLatencyBadge'),
    statResolutionBadge: document.getElementById('statResolutionBadge'),
    statPaletteBadge: document.getElementById('statPaletteBadge'),
    statShapesBadge: document.getElementById('statShapesBadge'),
    paletteSwatches: document.getElementById('paletteSwatches'),
    rngColors: document.getElementById('rngColors'),
    valColors: document.getElementById('valColors'),
    rngLineTol: document.getElementById('rngLineTol'),
    valLineTol: document.getElementById('valLineTol'),
    rngArcTol: document.getElementById('rngArcTol'),
    valArcTol: document.getElementById('valArcTol'),
    rngCurveTol: document.getElementById('rngCurveTol'),
    valCurveTol: document.getElementById('valCurveTol'),
    rngCorner: document.getElementById('rngCorner'),
    valCorner: document.getElementById('valCorner'),
    rngSnap: document.getElementById('rngSnap'),
    valSnap: document.getElementById('valSnap'),
    lblCutout: document.getElementById('lblCutout'),
    lblStacked: document.getElementById('lblStacked'),
    presetCards: document.querySelectorAll('.preset-card'),
    chkToggleAllPrimitives: document.getElementById('chkToggleAllPrimitives'),
    chkShowLines: document.getElementById('chkShowLines'),
    chkShowArcs: document.getElementById('chkShowArcs'),
    chkShowCircles: document.getElementById('chkShowCircles'),
    chkShowBeziers: document.getElementById('chkShowBeziers'),
    chkShowNodes: document.getElementById('chkShowNodes'),
    rngOverlayOpacity: document.getElementById('rngOverlayOpacity'),
    valOverlayOpacity: document.getElementById('valOverlayOpacity'),
    badgeCountLines: document.getElementById('badgeCountLines'),
    badgeCountArcs: document.getElementById('badgeCountArcs'),
    badgeCountCircles: document.getElementById('badgeCountCircles'),
    badgeCountBeziers: document.getElementById('badgeCountBeziers'),
    badgeCountNodes: document.getElementById('badgeCountNodes'),
    meterFillPrep: document.getElementById('meterFillPrep'),
    valMeterPrep: document.getElementById('valMeterPrep'),
    meterFillQuant: document.getElementById('meterFillQuant'),
    valMeterQuant: document.getElementById('valMeterQuant'),
    meterFillTopo: document.getElementById('meterFillTopo'),
    valMeterTopo: document.getElementById('valMeterTopo'),
    meterFillFit: document.getElementById('meterFillFit'),
    valMeterFit: document.getElementById('valMeterFit')
  };

  // Initialize
  async function init() {
    setupEventListeners();
    await loadDemoList();
    if (state.demos.length > 0) {
      loadDemoScene(state.demos[0].id);
    }
  }

  // Preset Configurations
  const PRESET_CONFIGS = {
    cad: {
      colors: 16,
      lineTol: 0.50,
      arcTol: 0.50,
      curveTol: 0.80,
      corner: 120,
      snap: 5.0,
      stacked: false
    },
    balanced: {
      colors: 16,
      lineTol: 0.80,
      arcTol: 0.70,
      curveTol: 0.80,
      corner: 135,
      snap: 2.0,
      stacked: false
    },
    artistic: {
      colors: 24,
      lineTol: 1.20,
      arcTol: 1.00,
      curveTol: 0.60,
      corner: 145,
      snap: 0.0,
      stacked: false
    }
  };

  function applyPreset(presetKey) {
    state.activePreset = presetKey;
    DOM.presetCards.forEach(c => {
      c.classList.toggle('active', c.dataset.preset === presetKey);
    });

    const cfg = PRESET_CONFIGS[presetKey] || PRESET_CONFIGS.balanced;
    state.params.colors = cfg.colors;
    state.params.lineTol = cfg.lineTol;
    state.params.arcTol = cfg.arcTol;
    state.params.curveTol = cfg.curveTol;
    state.params.corner = cfg.corner;
    state.params.snap = cfg.snap;
    state.params.stacked = cfg.stacked;

    updateControlUI();
    scheduleAutoVectorize();
  }

  function updateControlUI() {
    DOM.rngColors.value = state.params.colors;
    DOM.valColors.textContent = state.params.colors;

    DOM.rngLineTol.value = state.params.lineTol;
    DOM.valLineTol.textContent = Number(state.params.lineTol).toFixed(2) + ' px';

    DOM.rngArcTol.value = state.params.arcTol;
    DOM.valArcTol.textContent = Number(state.params.arcTol).toFixed(2) + ' px';

    DOM.rngCurveTol.value = state.params.curveTol;
    DOM.valCurveTol.textContent = Number(state.params.curveTol).toFixed(2) + ' px';

    DOM.rngCorner.value = state.params.corner;
    DOM.valCorner.textContent = Math.round(state.params.corner) + '°';

    DOM.rngSnap.value = state.params.snap;
    DOM.valSnap.textContent = Number(state.params.snap).toFixed(1) + '°';

    if (state.params.stacked) {
      DOM.lblStacked.classList.add('active');
      DOM.lblCutout.classList.remove('active');
      DOM.lblStacked.querySelector('input').checked = true;
    } else {
      DOM.lblCutout.classList.add('active');
      DOM.lblStacked.classList.remove('active');
      DOM.lblCutout.querySelector('input').checked = true;
    }
  }

  // Demo Scenes
  async function loadDemoList() {
    try {
      const res = await fetch('/api/demos');
      state.demos = await res.json();
      renderSceneButtons();
    } catch (e) {
      console.error('Failed to load demos:', e);
    }
  }

  function renderSceneButtons() {
    DOM.sceneButtons.innerHTML = '';
    state.demos.forEach(d => {
      const btn = document.createElement('button');
      btn.className = `scene-btn ${d.id === state.currentDemoId ? 'active' : ''}`;
      btn.dataset.id = d.id;
      btn.textContent = d.name;
      btn.title = d.desc;
      btn.addEventListener('click', () => loadDemoScene(d.id));
      DOM.sceneButtons.appendChild(btn);
    });
  }

  async function loadDemoScene(id) {
    state.currentDemoId = id;
    document.querySelectorAll('.scene-btn').forEach(b => {
      b.classList.toggle('active', b.dataset.id === id);
    });

    const demo = state.demos.find(d => d.id === id);
    if (demo && demo.preset && PRESET_CONFIGS[demo.preset]) {
      applyPreset(demo.preset);
    }

    try {
      const res = await fetch(`/api/demo/png?id=${id}`);
      const blob = await res.blob();
      loadImageBlob(blob);
    } catch (e) {
      console.error(`Failed to load demo scene ${id}:`, e);
    }
  }

  // Image Loading
  function loadImageBlob(blob) {
    state.currentImageBlob = blob;
    const url = URL.createObjectURL(blob);
    DOM.rasterImg.onload = () => {
      state.currentImageDimensions = {
        width: DOM.rasterImg.naturalWidth,
        height: DOM.rasterImg.naturalHeight
      };
      updateCanvasDimensions();
      resetTransform();
      vectorize();
    };
    DOM.rasterImg.src = url;
  }

  function updateCanvasDimensions() {
    const w = state.currentImageDimensions.width || 512;
    const h = state.currentImageDimensions.height || 512;
    DOM.frameRaster.style.width = `${w}px`;
    DOM.frameRaster.style.height = `${h}px`;
    DOM.frameVector.style.width = `${w}px`;
    DOM.frameVector.style.height = `${h}px`;
    DOM.statResolutionBadge.textContent = `${w} × ${h} px`;
  }

  // Vectorization Runner
  async function vectorize() {
    if (!state.currentImageBlob) return;

    DOM.btnVectorize.classList.add('loading');
    DOM.btnVectorize.innerHTML = '<span class="spinner"></span> Vectorizing...';

    const query = new URLSearchParams({
      preset: state.activePreset,
      colors: state.params.colors,
      line_tol: state.params.lineTol,
      arc_tol: state.params.arcTol,
      curve_tol: state.params.curveTol,
      corner: state.params.corner,
      snap: state.params.snap,
      stacked: state.params.stacked
    });

    try {
      const res = await fetch(`/api/vectorize?${query.toString()}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: state.currentImageBlob
      });

      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${await res.text()}`);
      }

      const data = await res.json();
      state.latestSvg = data.svg;
      state.latestOverlaySvg = data.overlay_svg;
      state.latestDxf = data.dxf;
      state.latestStats = data.stats;

      // Render vector result
      DOM.vectorSvgHost.innerHTML = data.svg;
      DOM.vectorOverlayHost.innerHTML = data.overlay_svg;

      // Apply overlay visibility & styles
      applyOverlayFilters();

      // Update statistics & meters
      updateStatsUI(data.stats);
    } catch (err) {
      console.error('Vectorization error:', err);
      alert('Vectorization failed: ' + err.message);
    } finally {
      DOM.btnVectorize.classList.remove('loading');
      DOM.btnVectorize.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg><span>Vectorize</span>';
    }
  }

  function updateStatsUI(s) {
    if (!s) return;
    DOM.statLatencyBadge.textContent = `⚡ ${Number(s.total_ms).toFixed(1)} ms`;
    DOM.statPaletteBadge.textContent = `${s.palette_count} Colors`;
    DOM.statShapesBadge.textContent = `${s.shapes} Shapes`;

    // Primitive counts
    DOM.badgeCountLines.textContent = s.lines;
    DOM.badgeCountArcs.textContent = s.arcs;
    DOM.badgeCountCircles.textContent = s.circles;
    DOM.badgeCountBeziers.textContent = s.beziers;
    DOM.badgeCountNodes.textContent = s.nodes;

    // Timing meters
    const total = Math.max(s.total_ms, 0.1);
    DOM.valMeterPrep.textContent = `${Number(s.prepare_ms).toFixed(1)} ms`;
    DOM.meterFillPrep.style.width = `${Math.min(100, (s.prepare_ms / total) * 100)}%`;

    DOM.valMeterQuant.textContent = `${Number(s.quantize_ms).toFixed(1)} ms`;
    DOM.meterFillQuant.style.width = `${Math.min(100, (s.quantize_ms / total) * 100)}%`;

    DOM.valMeterTopo.textContent = `${Number(s.topology_ms).toFixed(1)} ms`;
    DOM.meterFillTopo.style.width = `${Math.min(100, (s.topology_ms / total) * 100)}%`;

    DOM.valMeterFit.textContent = `${Number(s.fit_ms).toFixed(1)} ms`;
    DOM.meterFillFit.style.width = `${Math.min(100, (s.fit_ms / total) * 100)}%`;

    // Extract palette swatches from SVG
    extractPaletteSwatches();
  }

  function extractPaletteSwatches() {
    DOM.paletteSwatches.innerHTML = '';
    const fills = new Set();
    const paths = DOM.vectorSvgHost.querySelectorAll('path, circle, polygon');
    paths.forEach(el => {
      const f = el.getAttribute('fill');
      if (f && f !== 'none' && f.startsWith('#')) {
        fills.add(f.toLowerCase());
      }
    });

    fills.forEach(hex => {
      const s = document.createElement('div');
      s.className = 'palette-swatch';
      s.style.backgroundColor = hex;
      s.title = hex.toUpperCase();
      DOM.paletteSwatches.appendChild(s);
    });
  }

  // Primitive Inspector Overlay Filter
  function applyOverlayFilters() {
    const isMasterOn = DOM.chkToggleAllPrimitives.checked;
    DOM.vectorOverlayHost.style.display = isMasterOn ? 'block' : 'none';
    const opacity = DOM.rngOverlayOpacity.value / 100;
    DOM.vectorOverlayHost.style.opacity = opacity;

    const showLines = DOM.chkShowLines.checked;
    const showArcs = DOM.chkShowArcs.checked;
    const showCircles = DOM.chkShowCircles.checked;
    const showBeziers = DOM.chkShowBeziers.checked;
    const showNodes = DOM.chkShowNodes.checked;

    const overlaySvg = DOM.vectorOverlayHost.querySelector('svg');
    if (!overlaySvg) return;

    overlaySvg.querySelectorAll('.v-line').forEach(el => el.style.display = showLines ? '' : 'none');
    overlaySvg.querySelectorAll('path.v-arc').forEach(el => el.style.display = showArcs ? '' : 'none');
    overlaySvg.querySelectorAll('circle.v-arc').forEach(el => el.style.display = showCircles ? '' : 'none');
    overlaySvg.querySelectorAll('.v-bezier').forEach(el => el.style.display = showBeziers ? '' : 'none');
    overlaySvg.querySelectorAll('.v-node').forEach(el => el.style.display = showNodes ? '' : 'none');
  }

  // View Mode
  function setViewMode(mode) {
    state.viewMode = mode;
    DOM.viewModeGroup.querySelectorAll('.mode-btn').forEach(btn => {
      btn.classList.toggle('active', btn.dataset.mode === mode);
    });

    DOM.viewContainer.className = `view-container mode-${mode}`;
    if (mode === 'curtain') {
      updateCurtainPct(state.curtainPct);
    }
  }

  function updateCurtainPct(pct) {
    state.curtainPct = Math.max(0, Math.min(100, pct));
    DOM.viewContainer.style.setProperty('--curtain-pct', `${state.curtainPct}%`);
  }

  // Zoom & Pan Engine
  function applyTransform() {
    DOM.canvasStage.style.transform = `translate(${state.pan.x}px, ${state.pan.y}px) scale(${state.zoom})`;
    DOM.zoomLevelText.textContent = `${Math.round(state.zoom * 100)}%`;
  }

  function setZoom(newZoom, centerX, centerY) {
    const minZoom = 0.1;
    const maxZoom = 32.0;
    const clamped = Math.max(minZoom, Math.min(maxZoom, newZoom));
    if (clamped === state.zoom) return;

    // Zoom towards center point if provided
    if (centerX !== undefined && centerY !== undefined) {
      const rect = DOM.canvasViewport.getBoundingClientRect();
      const px = centerX - rect.left - rect.width / 2;
      const py = centerY - rect.top - rect.height / 2;

      state.pan.x = px - (px - state.pan.x) * (clamped / state.zoom);
      state.pan.y = py - (py - state.pan.y) * (clamped / state.zoom);
    }

    state.zoom = clamped;
    applyTransform();
  }

  function resetTransform() {
    state.zoom = 1.0;
    state.pan = { x: 0, y: 0 };
    applyTransform();
  }

  function fitToScreen() {
    const viewportRect = DOM.canvasViewport.getBoundingClientRect();
    const w = state.currentImageDimensions.width || 512;
    const h = state.currentImageDimensions.height || 512;
    const pad = 40;
    const scaleX = (viewportRect.width - pad) / (state.viewMode === 'split' ? w * 2 : w);
    const scaleY = (viewportRect.height - pad) / h;
    state.zoom = Math.min(scaleX, scaleY, 2.0);
    state.pan = { x: 0, y: 0 };
    applyTransform();
  }

  function scheduleAutoVectorize() {
    clearTimeout(state.autoVectorizeTimer);
    state.autoVectorizeTimer = setTimeout(() => {
      vectorize();
    }, 180);
  }

  // Export Download Trigger
  async function triggerExport(format) {
    if (!state.currentImageBlob) return;

    if (format === 'svg') {
      downloadBlob(new Blob([state.latestSvg], { type: 'image/svg+xml' }), 'vectorized.svg');
      return;
    }
    if (format === 'overlay') {
      downloadBlob(new Blob([state.latestOverlaySvg], { type: 'image/svg+xml' }), 'primitives_overlay.svg');
      return;
    }
    if (format === 'dxf') {
      downloadBlob(new Blob([state.latestDxf], { type: 'application/dxf' }), 'vectorized.dxf');
      return;
    }

    // PDF / EPS server export endpoints
    const query = new URLSearchParams({
      format: format,
      preset: state.activePreset,
      colors: state.params.colors,
      line_tol: state.params.lineTol,
      arc_tol: state.params.arcTol,
      curve_tol: state.params.curveTol,
      corner: state.params.corner,
      snap: state.params.snap,
      stacked: state.params.stacked
    });

    try {
      const res = await fetch(`/api/export?${query.toString()}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: state.currentImageBlob
      });
      if (!res.ok) throw new Error(await res.text());
      const blob = await res.blob();
      downloadBlob(blob, `vectorized.${format}`);
    } catch (e) {
      alert(`Export ${format.toUpperCase()} failed: ` + e.message);
    }
  }

  function downloadBlob(blob, filename) {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(a.href);
  }

  // Event Listeners Setup
  function setupEventListeners() {
    // Mode switcher buttons
    DOM.viewModeGroup.querySelectorAll('.mode-btn').forEach(b => {
      b.addEventListener('click', () => setViewMode(b.dataset.mode));
    });

    // Preset cards click
    DOM.presetCards.forEach(c => {
      c.addEventListener('click', () => applyPreset(c.dataset.preset));
    });

    // Zoom Buttons
    DOM.btnZoomIn.addEventListener('click', () => setZoom(state.zoom * 1.25));
    DOM.btnZoomOut.addEventListener('click', () => setZoom(state.zoom / 1.25));
    DOM.btnResetView.addEventListener('click', resetTransform);
    DOM.btnFitView.addEventListener('click', fitToScreen);

    // Mouse wheel zoom
    DOM.canvasViewport.addEventListener('wheel', (e) => {
      e.preventDefault();
      const factor = e.deltaY < 0 ? 1.15 : 0.87;
      setZoom(state.zoom * factor, e.clientX, e.clientY);
    }, { passive: false });

    // Pan via mouse drag
    DOM.canvasViewport.addEventListener('mousedown', (e) => {
      if (e.target.closest('#curtainDivider') || e.target.closest('#primitiveHud')) return;
      state.isPanning = true;
      DOM.canvasViewport.classList.add('panning');
      state.startPan = { x: e.clientX - state.pan.x, y: e.clientY - state.pan.y };
    });

    window.addEventListener('mousemove', (e) => {
      if (state.isPanning) {
        state.pan.x = e.clientX - state.startPan.x;
        state.pan.y = e.clientY - state.startPan.y;
        applyTransform();
      } else if (state.isDraggingCurtain) {
        const rect = DOM.viewContainer.getBoundingClientRect();
        const pct = ((e.clientX - rect.left) / rect.width) * 100;
        updateCurtainPct(pct);
      }
    });

    window.addEventListener('mouseup', () => {
      if (state.isPanning) {
        state.isPanning = false;
        DOM.canvasViewport.classList.remove('panning');
      }
      if (state.isDraggingCurtain) {
        state.isDraggingCurtain = false;
      }
    });

    // Curtain handle drag
    DOM.curtainDivider.addEventListener('mousedown', (e) => {
      e.stopPropagation();
      state.isDraggingCurtain = true;
    });

    // Sliders input events
    DOM.rngColors.addEventListener('input', () => {
      state.params.colors = parseInt(DOM.rngColors.value, 10);
      DOM.valColors.textContent = state.params.colors;
      scheduleAutoVectorize();
    });

    DOM.rngLineTol.addEventListener('input', () => {
      state.params.lineTol = parseFloat(DOM.rngLineTol.value);
      DOM.valLineTol.textContent = state.params.lineTol.toFixed(2) + ' px';
      scheduleAutoVectorize();
    });

    DOM.rngArcTol.addEventListener('input', () => {
      state.params.arcTol = parseFloat(DOM.rngArcTol.value);
      DOM.valArcTol.textContent = state.params.arcTol.toFixed(2) + ' px';
      scheduleAutoVectorize();
    });

    DOM.rngCurveTol.addEventListener('input', () => {
      state.params.curveTol = parseFloat(DOM.rngCurveTol.value);
      DOM.valCurveTol.textContent = state.params.curveTol.toFixed(2) + ' px';
      scheduleAutoVectorize();
    });

    DOM.rngCorner.addEventListener('input', () => {
      state.params.corner = parseFloat(DOM.rngCorner.value);
      DOM.valCorner.textContent = Math.round(state.params.corner) + '°';
      scheduleAutoVectorize();
    });

    DOM.rngSnap.addEventListener('input', () => {
      state.params.snap = parseFloat(DOM.rngSnap.value);
      DOM.valSnap.textContent = state.params.snap.toFixed(1) + '°';
      scheduleAutoVectorize();
    });

    // Topology Radio Cards
    DOM.lblCutout.addEventListener('click', () => {
      state.params.stacked = false;
      DOM.lblCutout.classList.add('active');
      DOM.lblStacked.classList.remove('active');
      scheduleAutoVectorize();
    });

    DOM.lblStacked.addEventListener('click', () => {
      state.params.stacked = true;
      DOM.lblStacked.classList.add('active');
      DOM.lblCutout.classList.remove('active');
      scheduleAutoVectorize();
    });

    // Primitive Inspector Toggles
    [
      DOM.chkToggleAllPrimitives,
      DOM.chkShowLines,
      DOM.chkShowArcs,
      DOM.chkShowCircles,
      DOM.chkShowBeziers,
      DOM.chkShowNodes
    ].forEach(chk => chk.addEventListener('change', applyOverlayFilters));

    DOM.rngOverlayOpacity.addEventListener('input', () => {
      DOM.valOverlayOpacity.textContent = `${DOM.rngOverlayOpacity.value}%`;
      applyOverlayFilters();
    });

    // Upload & Vectorize Buttons
    DOM.btnUpload.addEventListener('click', () => DOM.fileInput.click());
    DOM.fileInput.addEventListener('change', (e) => {
      if (e.target.files && e.target.files[0]) {
        loadImageBlob(e.target.files[0]);
      }
    });

    DOM.btnVectorize.addEventListener('click', () => vectorize());

    // Export Dropdown
    DOM.btnExportMenu.addEventListener('click', (e) => {
      e.stopPropagation();
      DOM.exportDropdown.parentElement.classList.toggle('open');
    });

    window.addEventListener('click', () => {
      DOM.exportDropdown.parentElement.classList.remove('open');
    });

    DOM.exportDropdown.querySelectorAll('.dropdown-item').forEach(item => {
      item.addEventListener('click', (e) => {
        e.stopPropagation();
        DOM.exportDropdown.parentElement.classList.remove('open');
        triggerExport(item.dataset.export);
      });
    });

    // Drag and Drop
    window.addEventListener('dragover', (e) => {
      e.preventDefault();
      DOM.dropOverlay.classList.add('active');
    });

    DOM.dropOverlay.addEventListener('dragleave', (e) => {
      e.preventDefault();
      DOM.dropOverlay.classList.remove('active');
    });

    DOM.dropOverlay.addEventListener('drop', (e) => {
      e.preventDefault();
      DOM.dropOverlay.classList.remove('active');
      if (e.dataTransfer.files && e.dataTransfer.files[0]) {
        loadImageBlob(e.dataTransfer.files[0]);
      }
    });
  }

  // Start app on DOMContentLoaded
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
