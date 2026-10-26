//! Embedded single-page frontend application for Groshe Web Companion.

pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Groshe — Web Companion</title>
  <style>
    :root {
      --bg: #121212;
      --surface: #1e1e1e;
      --card: #252525;
      --primary: #4caf50;
      --primary-dark: #2e7d32;
      --accent: #ff9800;
      --danger: #e53935;
      --text: #f5f5f5;
      --text-muted: #9e9e9e;
      --border: #333333;
      --radius: 12px;
      --font: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: var(--font);
      min-height: 100vh;
      display: flex;
      flex-direction: column;
    }
    header {
      background: var(--surface);
      border-bottom: 1px solid var(--border);
      padding: 12px 20px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      position: sticky;
      top: 0;
      z-index: 10;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 10px;
      font-size: 1.25rem;
      font-weight: 800;
    }
    .status-badge {
      font-size: 0.75rem;
      padding: 4px 10px;
      border-radius: 20px;
      background: #1b5e20;
      color: #a5d6a7;
      display: flex;
      align-items: center;
      gap: 6px;
    }
    .status-badge.offline {
      background: #4a1515;
      color: #ef9a9a;
    }
    .container {
      max-width: 720px;
      margin: 0 auto;
      width: 100%;
      padding: 20px 16px;
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    .list-picker-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }
    select {
      background: var(--surface);
      color: var(--text);
      border: 1px solid var(--border);
      padding: 10px 14px;
      border-radius: 8px;
      font-size: 1rem;
      outline: none;
      flex: 1;
    }
    .add-bar {
      display: flex;
      gap: 10px;
      background: var(--surface);
      padding: 8px;
      border-radius: var(--radius);
      border: 1px solid var(--border);
    }
    .add-bar input {
      flex: 1;
      background: transparent;
      border: none;
      color: var(--text);
      font-size: 1rem;
      padding: 8px 12px;
      outline: none;
    }
    .add-bar button {
      background: var(--primary);
      color: #ffffff;
      border: none;
      padding: 10px 18px;
      border-radius: 8px;
      font-weight: 700;
      cursor: pointer;
      min-height: 44px;
    }
    .add-bar button:hover { background: var(--primary-dark); }
    .items-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .item-card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 12px 16px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      min-height: 60px;
      transition: background 0.15s ease;
    }
    .item-card.checked {
      background: #171717;
      opacity: 0.65;
    }
    .item-left {
      display: flex;
      align-items: center;
      gap: 14px;
      flex: 1;
    }
    .checkbox {
      width: 28px;
      height: 28px;
      border: 2px solid var(--primary);
      border-radius: 6px;
      display: flex;
      align-items: center;
      justify-content: center;
      cursor: pointer;
      font-size: 1.1rem;
      color: var(--primary);
    }
    .item-card.checked .checkbox {
      background: var(--primary);
      color: #000;
    }
    .item-details {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .item-name {
      font-size: 1.05rem;
      font-weight: 600;
    }
    .item-card.checked .item-name {
      text-decoration: line-through;
      color: var(--text-muted);
    }
    .item-meta {
      font-size: 0.85rem;
      color: var(--accent);
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .category-chip {
      background: #333333;
      color: #cccccc;
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 0.75rem;
    }
    .photo-thumb {
      width: 36px;
      height: 36px;
      border-radius: 6px;
      object-fit: cover;
      border: 1px solid var(--border);
      cursor: pointer;
    }
    .item-actions {
      display: flex;
      align-items: center;
      gap: 4px;
    }
    .btn-edit, .btn-delete {
      background: transparent;
      border: none;
      font-size: 1.1rem;
      cursor: pointer;
      width: 32px;
      height: 32px;
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .btn-edit {
      color: var(--accent);
    }
    .btn-edit:hover { background: #1c2b3a; }
    .btn-delete {
      color: var(--danger);
      font-size: 1.25rem;
    }
    .btn-delete:hover { background: #3a1515; }
    .empty-state {
      text-align: center;
      padding: 40px 20px;
      color: var(--text-muted);
      font-size: 1rem;
    }
  </style>
</head>
<body>
  <header>
    <div class="brand">🛒 Groshe Web Companion</div>
    <div id="statusBadge" class="status-badge">● Live Synced</div>
  </header>

  <div class="container">
    <div class="list-picker-row">
      <select id="listSelect" onchange="changeList()">
        <option value="">Loading lists...</option>
      </select>
    </div>

    <form class="add-bar" onsubmit="handleAddItem(event)">
      <input type="text" id="itemInput" placeholder="Quick add (e.g. '2 gal milk @dairy', 'apples 3 lbs')..." autofocus>
      <button type="submit">+ Add</button>
    </form>

    <div class="items-list" id="itemsList">
      <div class="empty-state">Select a list to view items</div>
    </div>
  </div>

  <script>
    let activeListId = null;
    let ws = null;

    async function init() {
      try {
        const res = await fetch('/api/lists');
        const lists = await res.json();
        const select = document.getElementById('listSelect');
        select.innerHTML = '';
        if (lists.length === 0) {
          select.innerHTML = '<option value="">No local lists found</option>';
          return;
        }
        lists.forEach(l => {
          const opt = document.createElement('option');
          opt.value = l.id;
          opt.textContent = `${l.title} (${l.item_count} items)`;
          select.appendChild(opt);
        });
        activeListId = lists[0].id;
        loadActiveList();
      } catch (e) {
        console.error('Failed to load lists', e);
      }
    }

    function changeList() {
      activeListId = document.getElementById('listSelect').value;
      loadActiveList();
    }

    async function loadActiveList() {
      if (!activeListId) return;
      connectWebSocket(activeListId);
      try {
        const res = await fetch(`/api/lists/${activeListId}`);
        const data = await res.json();
        renderItems(data.items);
      } catch (e) {
        console.error('Failed to fetch list', e);
      }
    }

    function connectWebSocket(listId) {
      if (ws) ws.close();
      const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      ws = new WebSocket(`${proto}//${location.host}/api/ws/${listId}`);

      ws.onopen = () => {
        document.getElementById('statusBadge').className = 'status-badge';
        document.getElementById('statusBadge').textContent = '● Live Synced';
      };

      ws.onclose = () => {
        document.getElementById('statusBadge').className = 'status-badge offline';
        document.getElementById('statusBadge').textContent = '○ Reconnecting...';
        setTimeout(() => { if (activeListId === listId) connectWebSocket(listId); }, 2000);
      };

      ws.onmessage = (e) => {
        try {
          const msg = JSON.parse(e.data);
          if (msg.items) {
            renderItems(msg.items);
          }
        } catch (err) {}
      };
    }

    function renderItems(items) {
      const container = document.getElementById('itemsList');
      if (!items || items.length === 0) {
        container.innerHTML = '<div class="empty-state">List is empty. Add your first item above!</div>';
        return;
      }
      container.innerHTML = '';
      items.forEach(item => {
        const card = document.createElement('div');
        card.className = `item-card ${item.checked ? 'checked' : ''}`;

        const left = document.createElement('div');
        left.className = 'item-left';

        const checkbox = document.createElement('div');
        checkbox.className = 'checkbox';
        checkbox.textContent = item.checked ? '✓' : '';
        checkbox.onclick = () => toggleCheck(item.id, !item.checked);

        const details = document.createElement('div');
        details.className = 'item-details';

        const nameSpan = document.createElement('span');
        nameSpan.className = 'item-name';
        nameSpan.textContent = item.name;

        const metaSpan = document.createElement('span');
        metaSpan.className = 'item-meta';
        const qty = item.quantity ? `${item.quantity}${item.unit ? ' ' + item.unit : ''}` : '';
        metaSpan.textContent = qty;

        if (item.category) {
          const cat = document.createElement('span');
          cat.className = 'category-chip';
          cat.textContent = item.category;
          metaSpan.appendChild(cat);
        }

        details.appendChild(nameSpan);
        if (qty || item.category) details.appendChild(metaSpan);

        left.appendChild(checkbox);
        if (item.photo_hash) {
          const img = document.createElement('img');
          img.className = 'photo-thumb';
          img.src = `/api/photos/${item.photo_hash}`;
          img.alt = 'Thumbnail';
          left.appendChild(img);
        }
        left.appendChild(details);

        const actions = document.createElement('div');
        actions.className = 'item-actions';

        const btnEdit = document.createElement('button');
        btnEdit.className = 'btn-edit';
        btnEdit.innerHTML = '&#9998;';
        btnEdit.title = 'Edit item';
        btnEdit.onclick = () => editItemPrompt(item);

        const btnDelete = document.createElement('button');
        btnDelete.className = 'btn-delete';
        btnDelete.innerHTML = '&times;';
        btnDelete.title = 'Delete item';
        btnDelete.onclick = () => deleteItem(item.id);

        actions.appendChild(btnEdit);
        actions.appendChild(btnDelete);

        card.appendChild(left);
        card.appendChild(actions);
        container.appendChild(card);
      });
    }

    async function handleAddItem(e) {
      e.preventDefault();
      const input = document.getElementById('itemInput');
      const val = input.value.trim();
      if (!val || !activeListId) return;

      input.value = '';
      try {
        await fetch(`/api/lists/${activeListId}/items`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ raw_input: val })
        });
        loadActiveList();
      } catch (err) {
        console.error('Failed to add item', err);
      }
    }

    async function editItemPrompt(item) {
      if (!activeListId) return;
      const currentVal = (item.quantity ? item.quantity + (item.unit ? ' ' + item.unit : '') + ' ' : '') + item.name + (item.category ? ' @' + item.category : '');
      const input = prompt('Edit item (quick-add format: e.g. 2 gal milk @dairy):', currentVal);
      if (input === null) return;
      const val = input.trim();
      if (!val) return;
      try {
        await fetch(`/api/lists/${activeListId}/items/${item.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ raw_input: val })
        });
        loadActiveList();
      } catch (err) {
        console.error('Failed to update item', err);
      }
    }

    async function toggleCheck(itemId, checked) {
      if (!activeListId) return;
      try {
        await fetch(`/api/lists/${activeListId}/check/${itemId}`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ checked })\
        });
        loadActiveList();
      } catch (err) {\
        console.error('Failed to toggle check', err);
      }
    }

    async function deleteItem(itemId) {\
      if (!activeListId) return;
      try {
        await fetch(`/api/lists/${activeListId}/items/${itemId}`, { method: 'DELETE' });
        loadActiveList();
      } catch (err) {
        console.error('Failed to delete item', err);
      }
    }

    init();
  </script>
</body>
</html>
"#;
