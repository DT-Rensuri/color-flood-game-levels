const state = {
  templates: [],
  levels: [],
  activeTemplate: null,
  activeLevel: null,
};

const $ = (selector) => document.querySelector(selector);
const templateGrid = $('#templateGrid');
const levelGrid = $('#levelGrid');

function showToast(message) {
  const toast = $('#toast');
  toast.textContent = message;
  toast.classList.add('show');
  window.clearTimeout(showToast.timer);
  showToast.timer = window.setTimeout(() => toast.classList.remove('show'), 2600);
}

function parseJsonFile(file, kind) {
  return file.text().then((text) => {
    const data = JSON.parse(text.replace(/^\uFEFF/, ''));
    if (kind === 'template') {
      validateTemplate(data, file.name);
      return { name: file.name, path: file.webkitRelativePath || file.name, matrix: data };
    }
    validateLevel(data, file.name);
    return { name: file.name, path: file.webkitRelativePath || file.name, data };
  });
}

function validateTemplate(matrix, name) {
  if (!Array.isArray(matrix) || matrix.length === 0 || !matrix.every(Array.isArray)) {
    throw new Error(`${name}: template phải là ma trận JSON.`);
  }
  const width = matrix[0].length;
  if (!width || matrix.some((row) => row.length !== width || row.some((cell) => cell !== 0 && cell !== 1))) {
    throw new Error(`${name}: template phải là grid chữ nhật chỉ gồm 0/1.`);
  }
}

function validateLevel(level, name) {
  if (!level || !Array.isArray(level.grid) || !level.grid.length || !level.grid.every(Array.isArray)) {
    throw new Error(`${name}: level không có grid hợp lệ.`);
  }
  const width = level.grid[0].length;
  if (!width || level.grid.some((row) => row.length !== width)) {
    throw new Error(`${name}: grid level phải là hình chữ nhật.`);
  }
}

function displayName(item) {
  return item.path || item.name;
}

function renderFileList(items, container, selected, onSelect) {
  container.replaceChildren();
  container.classList.toggle('empty', items.length === 0);
  if (!items.length) {
    container.textContent = 'Chưa có dữ liệu hợp lệ.';
    return;
  }
  items.forEach((item, index) => {
    const button = document.createElement('button');
    button.className = `file-item${item === selected ? ' selected' : ''}`;
    button.textContent = displayName(item);
    button.title = displayName(item);
    button.addEventListener('click', () => onSelect(item, index));
    container.append(button);
  });
}

function renderTemplateList() {
  renderFileList(state.templates, $('#templateList'), state.activeTemplate, (item) => {
    state.activeTemplate = item;
    loadTemplateIntoEditor(item);
    renderTemplateList();
  });
}

function renderLevelList() {
  renderFileList(state.levels, $('#levelList'), state.activeLevel, (item) => {
    state.activeLevel = item;
    renderLevel(item);
    renderLevelList();
    activatePanel('levelPanel');
  });
}

function loadTemplateIntoEditor(item) {
  const matrix = item.matrix.map((row) => [...row]);
  $('#templateName').value = item.name;
  $('#templateTitle').textContent = item.name;
  $('#templatePath').textContent = item.path;
  $('#templateWidth').value = matrix[0].length;
  $('#templateHeight').value = matrix.length;
  state.activeTemplate = { ...item, matrix };
  renderTemplateGrid();
}

function newTemplate() {
  const width = Number($('#templateWidth').value) || 5;
  const height = Number($('#templateHeight').value) || 5;
  state.activeTemplate = {
    name: 'temp_new.json',
    path: 'new template',
    matrix: Array.from({ length: height }, () => Array(width).fill(0)),
  };
  $('#templateName').value = state.activeTemplate.name;
  $('#templateTitle').textContent = 'Template mới';
  $('#templatePath').textContent = 'Chưa lưu · tạo từ editor';
  renderTemplateGrid();
  renderTemplateList();
}

function renderTemplateGrid() {
  const item = state.activeTemplate;
  if (!item) return;
  const matrix = item.matrix;
  const height = matrix.length;
  const width = matrix[0].length;
  templateGrid.replaceChildren();
  templateGrid.style.gridTemplateColumns = `repeat(${width}, minmax(26px, 54px))`;
  matrix.forEach((row, y) => row.forEach((value, x) => {
    const cell = document.createElement('button');
    cell.className = `template-cell${value ? ' filled' : ''}`;
    cell.setAttribute('aria-label', `row ${y + 1}, column ${x + 1}, value ${value}`);
    cell.addEventListener('click', () => {
      item.matrix[y][x] = item.matrix[y][x] ? 0 : 1;
      renderTemplateGrid();
    });
    templateGrid.append(cell);
  }));
  $('#templateCellCount').textContent = width * height;
  $('#templateFilledCount').textContent = matrix.flat().filter(Boolean).length;
  $('#templateShape').textContent = `${width} × ${height}`;
}

function resizeTemplate() {
  if (!state.activeTemplate) newTemplate();
  const width = Math.max(1, Math.min(32, Number($('#templateWidth').value) || 1));
  const height = Math.max(1, Math.min(32, Number($('#templateHeight').value) || 1));
  const previous = state.activeTemplate.matrix;
  state.activeTemplate.matrix = Array.from({ length: height }, (_, y) =>
    Array.from({ length: width }, (_, x) => previous[y]?.[x] || 0),
  );
  $('#templateWidth').value = width;
  $('#templateHeight').value = height;
  renderTemplateGrid();
}

function saveTemplate() {
  if (!state.activeTemplate) newTemplate();
  const name = ($('#templateName').value.trim() || 'template.json').replace(/[^a-zA-Z0-9_.-]/g, '_');
  const filename = name.endsWith('.json') ? name : `${name}.json`;
  const blob = new Blob([JSON.stringify(state.activeTemplate.matrix, null, 4) + '\n'], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
  showToast(`Đã xuất ${filename}`);
}

function renderLevel(item) {
  const level = item.data;
  const rows = level.grid.length;
  const cols = level.grid[0].length;
  $('#levelTitle').textContent = level.level ? `Level ${level.level}` : item.name;
  $('#levelPath').textContent = item.path;
  $('#levelMeta').innerHTML = '';
  [
    ['Grid', `${cols} × ${rows}`],
    ['Moves', level.maxMoves ?? '—'],
    ['Target', level.targetColor ?? '—'],
  ].forEach(([label, value]) => {
    const pill = document.createElement('span');
    pill.className = 'meta-pill';
    pill.textContent = `${label}: ${value}`;
    $('#levelMeta').append(pill);
  });

  const colors = level.colors || {};
  $('#levelPalette').replaceChildren();
  Object.entries(colors).forEach(([index, color]) => {
    const item = document.createElement('div');
    item.className = 'palette-item';
    item.innerHTML = `<span class="swatch" style="background:${escapeHtml(color)}"></span><span>${index}${Number(index) === level.targetColor ? ' · target' : ''}</span>`;
    $('#levelPalette').append(item);
  });

  levelGrid.replaceChildren();
  levelGrid.style.gridTemplateColumns = `repeat(${cols}, minmax(12px, 42px))`;
  level.grid.forEach((row) => row.forEach((value) => {
    const cell = document.createElement('div');
    cell.className = 'level-cell';
    cell.title = `Color ${value}`;
    cell.style.background = colors[String(value)] || '#33465a';
    levelGrid.append(cell);
  }));
}

function escapeHtml(value) {
  return String(value).replace(/[&<>'"]/g, (char) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[char]));
}

function activatePanel(panelId) {
  document.querySelectorAll('.tab').forEach((tab) => tab.classList.toggle('active', tab.dataset.panel === panelId));
  document.querySelectorAll('.panel').forEach((panel) => panel.classList.toggle('active-panel', panel.id === panelId));
}

async function loadFiles(event, kind) {
  const files = [...event.target.files].filter((file) => file.name.toLowerCase().endsWith('.json'));
  const parsed = [];
  for (const file of files) {
    try {
      parsed.push(await parseJsonFile(file, kind));
    } catch (error) {
      console.warn(error);
      showToast(error.message);
    }
  }
  parsed.sort((a, b) => displayName(a).localeCompare(displayName(b), undefined, { numeric: true }));
  if (kind === 'template') {
    state.templates = parsed;
    renderTemplateList();
    if (parsed[0]) {
      state.activeTemplate = parsed[0];
      loadTemplateIntoEditor(parsed[0]);
    }
  } else {
    state.levels = parsed;
    renderLevelList();
    if (parsed[0]) {
      state.activeLevel = parsed[0];
      renderLevel(parsed[0]);
    }
  }
  if (parsed.length) showToast(`Đã mở ${parsed.length} file ${kind === 'template' ? 'template' : 'level'}.`);
}

document.querySelectorAll('.tab').forEach((tab) => tab.addEventListener('click', () => activatePanel(tab.dataset.panel)));
$('#templateFiles').addEventListener('change', (event) => loadFiles(event, 'template'));
$('#levelFiles').addEventListener('change', (event) => loadFiles(event, 'level'));
$('#newTemplate').addEventListener('click', newTemplate);
$('#resizeTemplate').addEventListener('click', resizeTemplate);
$('#saveTemplate').addEventListener('click', saveTemplate);

newTemplate();
