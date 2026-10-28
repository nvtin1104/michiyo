<script lang="ts">
  import { onMount } from 'svelte';
  type Device = { id: string; name: string; last_seen: number | null; revoked_at: number | null };
  type Key = { id: string; name: string; prefix: string; scopes: string[]; daily_limit: number | null; revoked_at: number | null };
  type Provider = { provider: string; updated_at: number };
  type Usage = { day: string; route: string; model: string; requests: number };
  type SyncPage = { records: Record<string, unknown>[]; cursor: string; more: boolean };
  let section = 'devices';
  let devices: Device[] = [], keys: Key[] = [], providers: Provider[] = [], usage: Usage[] = [];
  let error = '', message = '', deviceName = '', keyName = '', provider = 'openai', providerKey = '';
  let chatScope = true, embedScope = false, dailyLimit: number | undefined;
  let pairingCode = '', pairingExpiry = 0, createdKey = '';
  let busy = false;
  let dataType = 'note', dataPage: SyncPage | null = null, dataCursor = '0';
  const date = (value: number | null) => value ? new Date(value).toLocaleString('vi-VN') : 'Chưa hoạt động';

  async function api<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
    const response = await fetch(`/api/${path}`, { method, headers: body === undefined ? {} : { 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
    const result = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(response.status === 401 ? 'Phiên Cloudflare Access không hợp lệ.' : response.status === 503 ? 'Chưa kết nối dịch vụ Michiyo Cloud.' : `Không thực hiện được yêu cầu (HTTP ${response.status}).`);
    return result as T;
  }
  async function refresh() {
    const [d, k, p, u] = await Promise.all([
      api<{ devices: Device[] }>('admin/devices'), api<{ api_keys: Key[] }>('admin/api-keys'),
      api<{ provider_keys: Provider[] }>('admin/provider-keys'), api<{ usage: Usage[] }>('admin/usage?days=7'),
    ]);
    devices = d.devices; keys = k.api_keys; providers = p.provider_keys; usage = u.usage;
  }
  async function action(work: () => Promise<void>) {
    busy = true; error = ''; message = '';
    try { await work(); } catch (e) { error = e instanceof Error ? e.message : 'Có lỗi xảy ra.'; } finally { busy = false; }
  }
  onMount(() => { void action(refresh); });
  async function pair() {
    const result = await api<{ code: string; expires_at: number }>('pair/start', 'POST', { name: deviceName });
    pairingCode = result.code; pairingExpiry = result.expires_at; deviceName = '';
  }
  async function rename(d: Device) {
    const name = prompt('Tên thiết bị', d.name);
    if (name === null) return;
    await api(`admin/devices/${d.id}`, 'PATCH', { name }); await refresh();
  }
  async function revoke(path: string, name: string) {
    if (!confirm(`Thu hồi ${name}?`)) return;
    await api(path, 'DELETE'); await refresh();
  }
  async function saveProvider() {
    await api(`admin/provider-keys/${provider}`, 'PUT', { key: providerKey });
    providerKey = ''; message = 'Đã lưu khóa nhà cung cấp.'; await refresh();
  }
  async function createKey() {
    const scopes = [...(chatScope ? ['ai:chat'] : []), ...(embedScope ? ['ai:embed'] : [])];
    const limit = dailyLimit ?? null;
    if (!keyName.trim() || !scopes.length || (limit !== null && (!Number.isInteger(limit) || limit < 1))) throw new Error('Nhập tên, ít nhất một quyền và hạn mức hợp lệ.');
    const result = await api<{ key: string }>('admin/api-keys', 'POST', { name: keyName, scopes, daily_limit: limit });
    createdKey = result.key; keyName = ''; dailyLimit = undefined; await refresh();
  }
  async function loadData(next = false) {
    const cursor = next && dataPage ? dataPage.cursor : '0';
    dataPage = null;
    dataPage = await api<SyncPage>(`admin/sync/records?type=${encodeURIComponent(dataType)}&cursor=${cursor}`);
    dataCursor = cursor;
  }
  async function exportData() {
    const records: Record<string, unknown>[] = [];
    let cursor = '0', more = true;
    while (more) {
      const page = await api<SyncPage>(`admin/sync/records?type=${encodeURIComponent(dataType)}&cursor=${cursor}`);
      records.push(...page.records); more = page.more;
      if (more && (page.cursor === cursor || records.length >= 10000)) throw new Error('Không thể xuất đầy đủ: dữ liệu vượt giới hạn hoặc cursor không tiến.');
      cursor = page.cursor;
    }
    const objectUrl = URL.createObjectURL(new Blob([JSON.stringify({ type: dataType, records }, null, 2)], { type: 'application/json' }));
    const link = document.createElement('a'); link.href = objectUrl; link.download = `michiyo-${dataType}.json`; link.click();
    setTimeout(() => URL.revokeObjectURL(objectUrl), 1000);
  }
</script>

<svelte:head><title>Michiyo Admin</title><meta name="robots" content="noindex,nofollow" /></svelte:head>
<div class="layout">
  <aside><div class="brand">🌱 Michiyo <small>admin</small></div><nav aria-label="Quản trị">
    {#each [['overview', 'Tổng quan'], ['devices', 'Thiết bị'], ['api', 'API'], ['usage', 'Sử dụng'], ['data', 'Dữ liệu']] as [id, label]}
      <button class:active={section === id} onclick={() => { section = id; error = ''; message = ''; if (id === 'data' && !dataPage) void action(() => loadData()); }}>{label}</button>
    {/each}
  </nav></aside>
  <main>
    <header><div><span class="eyebrow">MICHIYO CLOUD</span><h1>{section === 'overview' ? 'Tổng quan' : section === 'devices' ? 'Thiết bị' : section === 'api' ? 'Quản lý API' : section === 'data' ? 'Dữ liệu' : 'Sử dụng'}</h1></div><button class="secondary" disabled={busy} onclick={() => action(async () => { await refresh(); if (section === 'data') await loadData(); })}>Làm mới</button></header>
    {#if error}<p class="notice error" role="alert">{error}</p>{/if}
    {#if message}<p class="notice" role="status">{message}</p>{/if}
    {#if section === 'overview'}
      <section class="card"><h2>Hoạt động gần đây</h2><div class="item"><span>Thiết bị hoạt động trong 2 phút</span><strong>{devices.filter((d) => !d.revoked_at && d.last_seen !== null && Date.now() - d.last_seen < 120000).length}</strong></div><div class="item"><span>Lượt AI trong 7 ngày gần đây</span><strong>{usage.reduce((sum, row) => sum + row.requests, 0)}</strong></div><p>Chi phí, lỗi và tin nhắn bot chưa có dữ liệu.</p></section>
    {:else if section === 'devices'}
      <section class="card"><h2>Ghép nối thiết bị</h2><p>Tạo mã và nhập mã đó trong cài đặt Michiyo trên desktop.</p>
        <form onsubmit={(e) => { e.preventDefault(); void action(pair); }}><label for="device-name">Tên thiết bị</label><div class="row"><input id="device-name" bind:value={deviceName} maxlength="60" placeholder="Máy tính của tôi" /><button disabled={busy}>Tạo mã</button></div></form>
        {#if pairingCode}<div class="secret"><strong>{pairingCode}</strong><span>Hết hạn: {date(pairingExpiry)}</span><button class="secondary" onclick={() => pairingCode = ''}>Đóng</button></div>{/if}
      </section>
      <section class="card"><h2>Thiết bị đã ghép nối</h2>{#if !devices.length}<p>Chưa có thiết bị.</p>{/if}
        {#each devices as d (d.id)}<div class="item"><div><strong>{d.name}</strong><p>{d.revoked_at ? 'Đã thu hồi' : `Hoạt động gần nhất: ${date(d.last_seen)}`}</p></div>{#if !d.revoked_at}<div class="row"><button class="secondary" disabled={busy} onclick={() => action(() => rename(d))}>Đổi tên</button><button class="danger" disabled={busy} onclick={() => action(() => revoke(`admin/devices/${d.id}`, d.name))}>Thu hồi</button></div>{/if}</div>{/each}
      </section>
    {:else if section === 'api'}
      <section class="card"><h2>Khóa nhà cung cấp</h2><p>Giá trị khóa chỉ được gửi khi lưu và không thể xem lại.</p><form onsubmit={(e) => { e.preventDefault(); void action(saveProvider); }}>
        <label for="provider">Nhà cung cấp</label><select id="provider" bind:value={provider}><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option><option value="openrouter">OpenRouter</option></select>
        <label for="provider-key">Khóa mới</label><input id="provider-key" type="password" bind:value={providerKey} required maxlength="500" autocomplete="off" /><button disabled={busy}>Lưu khóa</button>
      </form>{#each providers as p (p.provider)}<div class="item"><strong>{p.provider}</strong><button class="danger" disabled={busy} onclick={() => action(() => revoke(`admin/provider-keys/${p.provider}`, p.provider))}>Xóa</button></div>{/each}</section>
      <section class="card"><h2>Khóa Michiyo</h2><form onsubmit={(e) => { e.preventDefault(); void action(createKey); }}><label for="key-name">Tên khóa</label><input id="key-name" bind:value={keyName} required maxlength="60" /><div class="checks"><label><input type="checkbox" bind:checked={chatScope} /> Chat</label><label><input type="checkbox" bind:checked={embedScope} /> Embedding</label></div><label for="limit">Hạn mức mỗi ngày (để trống nếu không giới hạn)</label><input id="limit" type="number" min="1" step="1" bind:value={dailyLimit} /><button disabled={busy}>Tạo khóa</button></form>
        {#if createdKey}<div class="secret"><span>Chỉ hiển thị một lần. Hãy sao chép và lưu an toàn.</span><code>{createdKey}</code><button class="secondary" onclick={() => createdKey = ''}>Đã lưu</button></div>{/if}
        {#each keys as k (k.id)}<div class="item"><div><strong>{k.name}</strong><p>{k.prefix}… · {k.scopes.join(', ')} {k.revoked_at ? '· Đã thu hồi' : ''}</p></div>{#if !k.revoked_at}<button class="danger" disabled={busy} onclick={() => action(() => revoke(`admin/api-keys/${k.id}`, k.name))}>Thu hồi</button>{/if}</div>{/each}
      </section>
    {:else if section === 'data'}
      <section class="card"><h2>Dữ liệu đồng bộ</h2><p>Chỉ xem dữ liệu hiện có trên cloud. Xuất JSON có thể phản ánh thay đổi đang diễn ra trong lúc tải.</p>
        <label for="data-type">Loại dữ liệu</label><select id="data-type" bind:value={dataType} disabled={busy} onchange={() => action(() => loadData())}><option value="note">Ghi chú</option><option value="habit">Thói quen</option><option value="habit_log">Lần check-in</option><option value="event">Lịch</option><option value="reminder">Nhắc nhở</option></select>
        <div class="row" style="margin-top: 16px"><button class="secondary" disabled={busy} onclick={() => action(exportData)}>Xuất JSON</button>{#if dataCursor !== '0'}<button class="secondary" disabled={busy} onclick={() => action(() => loadData())}>Về đầu</button>{/if}{#if dataPage?.more}<button disabled={busy} onclick={() => action(() => loadData(true))}>Trang tiếp</button>{/if}</div>
        {#if dataPage && !dataPage.records.length}<p>Không có dữ liệu.</p>{/if}
        {#each dataPage?.records ?? [] as record}<pre>{JSON.stringify(record, null, 2)}</pre>{/each}
      </section>
    {:else}
      <section class="card"><h2>7 ngày gần đây</h2>{#if !usage.length}<p>Chưa có lượt sử dụng.</p>{/if}{#each usage as u}<div class="item"><div><strong>{u.day} · {u.route}</strong><p>{u.model}</p></div><strong>{u.requests} lượt</strong></div>{/each}</section>
    {/if}
  </main>
</div>

<style>
  :global(*){box-sizing:border-box}:global(body){margin:0;font:15px system-ui,sans-serif;color:#24362d;background:#f3f6ef} .layout{display:flex;min-height:100vh} aside{width:220px;background:#20392c;color:white;padding:30px 18px;flex-shrink:0}.brand{font-size:23px;font-weight:750;margin-bottom:34px}.brand small{font-size:12px;color:#b5d5bd}nav{display:grid;gap:7px}nav button{text-align:left;color:#dcecdf;background:transparent}nav button.active{background:#456b50;color:white}main{width:min(900px,100%);padding:38px 48px}header{display:flex;justify-content:space-between;align-items:center;margin-bottom:24px}h1{font-size:30px;margin:4px 0}h2{font-size:19px;margin:0 0 8px}.eyebrow{font-size:11px;letter-spacing:.14em;color:#6e8877;font-weight:700}.card{background:#fff;border:1px solid #dce7da;border-radius:14px;padding:24px;margin:0 0 18px;box-shadow:0 4px 18px #23452b0a}p{color:#667a6b;line-height:1.5;margin:6px 0 16px}label{display:block;font-weight:600;margin:14px 0 6px}input:not([type=checkbox]),select{width:100%;padding:10px;border:1px solid #cbd9cb;border-radius:7px;font:inherit;background:white}button{border:0;border-radius:7px;padding:10px 14px;background:#367347;color:#fff;font:inherit;font-weight:600;cursor:pointer}button:disabled{opacity:.5;cursor:default}.secondary{background:#eaf1e9;color:#34543b}.danger{background:#fff0ee;color:#a53232}form>button{margin-top:15px}.row{display:flex;gap:8px;flex-wrap:wrap}.row input{flex:1;min-width:0}.item{border-top:1px solid #edf2eb;display:flex;justify-content:space-between;align-items:center;gap:12px;padding:14px 0}.item p{margin:4px 0 0;font-size:13px}pre{background:#f3f7f1;border-radius:7px;padding:12px;white-space:pre-wrap;overflow-wrap:anywhere}.secret{display:grid;gap:10px;margin-top:16px;padding:15px;background:#eef8e9;border-radius:8px;overflow-wrap:anywhere}.secret strong{font-size:24px;letter-spacing:.15em}.secret button{justify-self:start}.checks{display:flex;gap:20px}.checks label{font-weight:400}.notice{padding:12px;background:#e6f4e7;border-radius:8px}.error{background:#fff0ee;color:#9d2929}@media(max-width:650px){.layout{display:block}aside{width:auto;padding:18px}.brand{margin:0 0 14px}nav{display:flex;flex-wrap:wrap}main{padding:24px 16px}.item{align-items:flex-start;flex-wrap:wrap}}
</style>
