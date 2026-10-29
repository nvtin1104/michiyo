<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { call, desktop, type Config, defaults } from '../lib/api';
  import { onMount } from 'svelte';
  import Select from '../lib/Select.svelte';
  const choices = (values: string[]) =>
    values.map((value) => ({ value, label: value }));
  const languages = (auto = false) => [
    ...(auto
      ? [
          {
            value: 'auto',
            label: $locale === 'en' ? 'Detect automatically' : 'Tự nhận diện',
          },
        ]
      : []),
    ...Object.entries({
      vi: 'Tiếng Việt',
      en: 'English',
      ja: '日本語',
      zh: '中文',
      ko: '한국어',
      fr: 'Français',
      de: 'Deutsch',
    }).map(([value, label]) => ({ value, label })),
  ];
  export let config: Config;
  export let save: () => Promise<void>;
  let tab = 'Chung';
  let status = '';
  let keys: Record<string, string> = {
    claude: '',
    google: '',
    libre: '',
    openai: '',
    cloudCode: '',
  };
  let models: Record<string, string[]> = { external: [], local: [] };
  const presets: [string, string][] = [
    ['OpenAI', 'https://api.openai.com/v1'],
    ['OpenRouter', 'https://openrouter.ai/api/v1'],
    ['Ollama', 'http://127.0.0.1:11434/v1'],
    ['LM Studio', 'http://127.0.0.1:1234/v1'],
  ];
  type SyncStatus = {
    enabled: boolean;
    pending: number;
    last_sync_at: number | null;
    last_error: string | null;
    syncing: boolean;
  };
  let sync: SyncStatus | null = null;
  async function loadSync() {
    if (!desktop) return;
    try {
      sync = await call<SyncStatus>('sync_status');
    } catch {
      sync = null;
    }
  }
  async function syncNow() {
    try {
      await save();
      sync = await call<SyncStatus>('sync_now');
      status = 'Đã đồng bộ';
    } catch (e) {
      status = String(e);
      await loadSync();
    }
  }
  async function pairCloud() {
    try {
      await save();
      await call('cloud_pair', {
        url: config.cloudUrl,
        code: keys.cloudCode,
      });
      keys.cloudCode = '';
      stored = await call('secret_status');
      status = 'Đã ghép nối với Michiyo Cloud';
    } catch (e) {
      status = String(e);
    }
  }
  async function unpairCloud() {
    try {
      await call('save_secret', { name: 'cloud', value: '' });
      stored = await call('secret_status');
      status = 'Đã ngắt kết nối Michiyo Cloud';
    } catch (e) {
      status = String(e);
    }
  }
  async function loadModels(target: 'external' | 'local') {
    try {
      await save();
      models[target] = await call<string[]>('ai_models', { target });
      status = `Tìm thấy ${models[target].length} model`;
    } catch (e) {
      status = String(e);
    }
  }
  let show = false;
  let stored: Record<string, boolean> = {};
  let dirtyTimer: ReturnType<typeof setTimeout>;
  let skins = ['reze', 'moon-gardener'];
  let skinDirectory = '';
  onMount(() => {
    if (desktop) {
      call<Record<string, boolean>>('secret_status').then((v) => (stored = v));
      call<{ names: string[]; directory: string }>('list_skins').then((v) => {
        skins = [...new Set(['reze', 'moon-gardener', ...v.names.filter((name) => name !== 'default')])];
        skinDirectory = v.directory;
      });
    }
    return () => clearTimeout(dirtyTimer);
  });
  function changed() {
    clearTimeout(dirtyTimer);
    dirtyTimer = setTimeout(async () => {
      try {
        await save();
        status = 'Đã lưu thay đổi';
      } catch (e) {
        status = String(e);
      }
    }, 500);
  }
  async function saveKey(name: string) {
    try {
      await call('save_secret', { name, value: keys[name] });
      keys[name] = '';
      stored = await call('secret_status');
      status = 'Đã cập nhật key trong OS keychain';
    } catch (e) {
      status = String(e);
    }
  }
</script>

<div class="settings-shell">
  <header class="settings-header">
    <h1>{t('Cài đặt', $locale)}</h1>
    <span>{t('Tùy chỉnh trải nghiệm của bạn', $locale)}</span>
  </header>
  <div class="settings-layout">
    <nav aria-label="Nhóm cài đặt">
      {#each ['Chung', 'API', 'AI', 'Đồng bộ', 'Chat', 'Dịch', 'Phím tắt', 'Giao diện'] as name}<button
          class:active={tab === name}
          onclick={() => (tab = name)}>{t(name, $locale)}</button
        >{/each}
    </nav>
    <section
      class="settings-content"
      aria-label={t(tab, $locale)}
      onchange={changed}
    >
      <h2>{t(tab, $locale)}</h2>
      {#if tab === 'Chung'}<label
          >{t('Ngôn ngữ giao diện', $locale)}<Select
            label={t('Ngôn ngữ giao diện', $locale)}
            bind:value={config.locale}
            options={[
              { value: 'vi', label: 'Tiếng Việt' },
              { value: 'en', label: 'English' },
            ]}
            onchange={changed}
          /></label
        ><label class="check"
          ><input type="checkbox" bind:checked={config.autostart} />{t(
            'Khởi động cùng hệ thống',
            $locale,
          )}</label
        ><label
          >{t('Kích thước mascot', $locale)}<Select
            label={t('Kích thước mascot', $locale)}
            value={String(config.scale)}
            options={[1, 2, 3].map((value) => ({
              value: String(value),
              label: `${value}×`,
            }))}
            onchange={(value) => {
              config.scale = Number(value);
              changed();
            }}
          /></label
        ><label
          >{t('Độ trong suốt', $locale)}<input
            type="range"
            min="0.3"
            max="1"
            step="0.05"
            bind:value={config.opacity}
          /></label
        ><button onclick={() => call('reset_position')}
          >{t('Về vị trí mặc định', $locale)}</button
        >
        <label
          >{t('Vị trí mặc định', $locale)}<Select
            label={t('Vị trí mặc định', $locale)}
            bind:value={config.anchor}
            options={[
              {
                value: 'bottom-right',
                label: $locale === 'en' ? 'Bottom right' : 'Dưới phải',
              },
              {
                value: 'bottom-left',
                label: $locale === 'en' ? 'Bottom left' : 'Dưới trái',
              },
              {
                value: 'top-right',
                label: $locale === 'en' ? 'Top right' : 'Trên phải',
              },
              {
                value: 'top-left',
                label: $locale === 'en' ? 'Top left' : 'Trên trái',
              },
            ]}
            onchange={changed}
          /></label
        >
      {:else if tab === 'API'}<label class="check"
          ><input type="checkbox" bind:checked={show} />{t(
            'Hiện khóa đang nhập',
            $locale,
          )}</label
        >{#each ['claude', 'google', 'libre'] as name}<div class="key-card">
            <h3>
              {name === 'claude'
                ? 'Claude'
                : name === 'google'
                  ? 'Google Cloud'
                  : 'LibreTranslate'}
              <small>{stored[name] ? '● Đã có key' : '○ Chưa có key'}</small>
            </h3>
            <input
              aria-label={`${name} API key`}
              type={show ? 'text' : 'password'}
              bind:value={keys[name]}
              placeholder={t('Nhập key mới · để trống + Lưu để xóa', $locale)}
              autocomplete="off"
            />
            <div class="actions">
              <button onclick={() => saveKey(name)}
                >{t('Lưu key', $locale)}</button
              ><button
                onclick={async () => {
                  try {
                    status = await call<string>('test_connection', { name });
                  } catch (e) {
                    status = String(e);
                  }
                }}>{t('Kiểm tra kết nối', $locale)}</button
              >
            </div>
          </div>{/each}<label
          >{t('LibreTranslate URL', $locale)}<input
            bind:value={config.libreUrl}
          /></label
        >
        <p class="muted">
          {t('Khóa chỉ được lưu trong kho bảo mật của hệ điều hành.', $locale)}
        </p>
      {:else if tab === 'AI'}<label
          >Chế độ<Select
            label="Chế độ"
            bind:value={config.mode}
            options={[
              { value: 'online', label: 'Online' },
              { value: 'hybrid', label: 'Hybrid (Online, dự phòng cục bộ)' },
              { value: 'offline', label: 'Offline (chỉ model trên máy)' },
            ]}
            onchange={changed}
          /></label
        >
        <p class="muted">
          Offline không gửi chat hay bản dịch ra ngoài máy; Hybrid dùng model cục
          bộ khi model chính lỗi.
        </p>
        <label
          >Nguồn model<Select
            label="Nguồn model"
            bind:value={config.modelSource}
            options={[
              { value: 'external', label: 'Model ngoài' },
              { value: 'cloudflare', label: 'Model Cloudflare' },
            ]}
            onchange={changed}
          /></label
        >
        {#if config.modelSource === 'cloudflare'}
          <label
            >Địa chỉ Michiyo Cloud<input
              aria-label="Địa chỉ Michiyo Cloud"
              bind:value={config.cloudUrl}
              placeholder="https://michiyo.example.workers.dev"
            /></label
          >
          <label
            >Model Cloudflare (để trống = mặc định)<input
              aria-label="Model Cloudflare"
              bind:value={config.cloudModel}
              placeholder="@cf/meta/llama-3.1-8b-instruct"
            /></label
          >
          <div class="key-card">
            <h3>
              Ghép nối thiết bị <small
                >{stored.cloud ? '● Đã ghép nối' : '○ Chưa ghép nối'}</small
              >
            </h3>
            {#if !stored.cloud}
              <input
                aria-label="Mã ghép nối"
                bind:value={keys.cloudCode}
                placeholder="Mã từ trang quản trị (hiệu lực 10 phút)"
                autocomplete="off"
              />
            {/if}
            <div class="actions">
              {#if stored.cloud}<button onclick={unpairCloud}
                  >Ngắt kết nối</button
                >{:else}<button
                  disabled={!keys.cloudCode.trim() || !config.cloudUrl.trim()}
                  onclick={pairCloud}>Ghép nối</button
                >{/if}
            </div>
          </div>
        {:else}
          <label
            >Nhà cung cấp chat<Select
              label="Nhà cung cấp chat"
              bind:value={config.chatProvider}
              options={[
                { value: 'claude', label: 'Claude (Agent SDK)' },
                { value: 'openai_compat', label: 'OpenAI-compatible' },
              ]}
              onchange={changed}
            /></label
          >
          {#if config.chatProvider === 'openai_compat'}
            <div class="actions" role="group" aria-label="Preset">
              {#each presets as [name, url]}<button
                  onclick={() => {
                    config.openaiBase = url;
                    changed();
                  }}>{name}</button
                >{/each}
            </div>
            <label
              >Địa chỉ API<input
                aria-label="Địa chỉ API"
                bind:value={config.openaiBase}
              /></label
            >
            <label
              >Model<input
                aria-label="Model ngoài"
                list="models-external"
                bind:value={config.openaiModel}
              /><datalist id="models-external"
                >{#each models.external as id}<option value={id}></option
                  >{/each}</datalist
              ></label
            >
            <div class="key-card">
              <h3>
                API key <small
                  >{stored.openai ? '● Đã có key' : '○ Chưa có key'}</small
                >
              </h3>
              <input
                aria-label="openai API key"
                type="password"
                bind:value={keys.openai}
                placeholder={t('Nhập key mới · để trống + Lưu để xóa', $locale)}
                autocomplete="off"
              />
              <div class="actions">
                <button onclick={() => saveKey('openai')}
                  >{t('Lưu key', $locale)}</button
                ><button onclick={() => loadModels('external')}
                  >Liệt kê model</button
                >
              </div>
            </div>
          {/if}
        {/if}
        <h3>Model cục bộ</h3>
        <label
          >Địa chỉ cục bộ<input
            aria-label="Địa chỉ cục bộ"
            bind:value={config.localBase}
          /></label
        >
        <label
          >Model cục bộ<input
            aria-label="Model cục bộ"
            list="models-local"
            bind:value={config.localModel}
          /><datalist id="models-local"
            >{#each models.local as id}<option value={id}></option>{/each}</datalist
          ></label
        >
        <div class="actions">
          <button onclick={() => loadModels('local')}
            >Kiểm tra kết nối</button
          >
        </div>
      {:else if tab === 'Đồng bộ'}
        {#await loadSync()}{/await}
        <p class="muted">
          Đồng bộ ghi chú, thói quen, lịch và hoạt động giữa các thiết bị qua
          Michiyo Cloud. Chế độ Offline tắt hoàn toàn đồng bộ. Lịch sử trò chuyện
          không bao giờ được đồng bộ.
        </p>
        <div class="key-card">
          <h3>
            Trạng thái <small
              >{sync?.enabled ? '● Đang bật' : '○ Đang tắt'}</small
            >
          </h3>
          {#if sync?.enabled}
            <p>Thay đổi chờ gửi: {sync.pending}</p>
            <p>
              Lần đồng bộ cuối: {sync.last_sync_at
                ? new Date(sync.last_sync_at).toLocaleString()
                : 'chưa có'}
            </p>
          {:else}
            <p class="muted">
              Cần chế độ Online/Hybrid, địa chỉ Michiyo Cloud và thiết bị đã ghép
              nối (tab AI).
            </p>
          {/if}
          {#if sync?.last_error}<p class="error" role="alert">
              {sync.last_error}
            </p>{/if}
          <div class="actions">
            <button disabled={!sync?.enabled || sync.syncing} onclick={syncNow}
              >Đồng bộ ngay</button
            >
          </div>
        </div>
        <h3>Phạm vi</h3>
        {#each [['notes', 'Ghi chú'], ['habits', 'Thói quen'], ['calendar', 'Lịch và nhắc nhở'], ['activity', 'Lịch sử hoạt động']] as [name, label]}<label
            class="check"
            ><input
              type="checkbox"
              aria-label={label}
              bind:checked={config.syncScopes[name as keyof typeof config.syncScopes]}
            />{label}</label
          >{/each}
      {:else if tab === 'Chat'}<label
          >Model<Select
            label={t('Model', $locale)}
            bind:value={config.model}
            options={choices(['claude-haiku-5-5', 'claude-sonnet-5-5'])}
            onchange={changed}
          /></label
        ><label
          >System prompt<textarea
            rows="4"
            spellcheck="false"
            bind:value={config.systemPrompt}></textarea></label
        ><label
          >Max tokens<input
            type="number"
            min="1"
            max="16384"
            bind:value={config.maxTokens}
          /></label
        ><label
          >{t('Số hội thoại lưu', $locale)}<input
            type="number"
            min="1"
            max="100"
            bind:value={config.historyLimit}
          /></label
        >
      {:else if tab === 'Dịch'}<div class="columns">
          <label
            >{t('Nguồn', $locale)}<Select
              label={t('Nguồn', $locale)}
              bind:value={config.source}
              options={languages(true)}
              onchange={changed}
            /></label
          ><label
            >{t('Đích', $locale)}<Select
              label={t('Đích', $locale)}
              bind:value={config.target}
              options={languages()}
              onchange={changed}
            /></label
          ><label
            >{t('Đích thay thế', $locale)}<Select
              label={t('Đích thay thế', $locale)}
              bind:value={config.alternate}
              options={languages()}
              onchange={changed}
            /></label
          >
        </div>
        <label
          >{t('Lĩnh vực', $locale)}<input bind:value={config.domain} /></label
        ><label
          >{t('Giọng văn', $locale)}<Select
            label={t('Giọng văn', $locale)}
            bind:value={config.tone}
            options={choices(['Tự nhiên', 'Trang trọng', 'Thân mật'])}
            onchange={changed}
          /></label
        >
        <h3>{t('Thứ tự engine', $locale)}</h3>
        {#each config.providers as provider, i}<div class="provider-row">
            <span>0{i + 1} / {provider}</span><button
              disabled={i === 0}
              onclick={() => {
                const a = [...config.providers];
                [a[i - 1], a[i]] = [a[i], a[i - 1]];
                config.providers = a;
                changed();
              }}>↑</button
            >
          </div>{/each}<label class="check"
          ><input type="checkbox" bind:checked={config.fallback} />{t(
            'Tự chuyển engine khi gặp lỗi',
            $locale,
          )}</label
        ><label class="check"
          ><input type="checkbox" bind:checked={config.clipboardWatch} />{t(
            'Dịch khi copy (văn bản sẽ gửi tới engine đã chọn)',
            $locale,
          )}</label
        >
      {:else if tab === 'Phím tắt'}<label
          >{t('Dịch văn bản đã chọn', $locale)}<input
            bind:value={config.translateShortcut}
          /></label
        ><label
          >{t('Mở Chat', $locale)}<input
            bind:value={config.chatShortcut}
          /></label
        ><label>Mở Michiyo<input bind:value={config.shellShortcut} /></label
        ><label
          >{t('Hiện / Ẩn mascot', $locale)}<input
            bind:value={config.toggleShortcut}
          /></label
        >
        <p class="muted">
          {t(
            'Ví dụ: Ctrl+Alt+T. Lỗi đăng ký hoặc xung đột sẽ hiển thị bên dưới.',
            $locale,
          )}
        </p>
      {:else}<label
          >{t('Skin', $locale)}<Select
            label={t('Skin', $locale)}
            bind:value={config.skin}
            options={skins.map((value) => ({
              value,
              label:
                value === 'moon-gardener'
                  ? 'Moon gardener'
                  : value === 'reze'
                    ? 'Reze (pixel)'
                  : value,
            }))}
            onchange={changed}
          /></label
        ><label
          >{t('Tốc độ animation', $locale)}<input
            type="range"
            min="0.5"
            max="2"
            step="0.25"
            bind:value={config.animationSpeed}
          /></label
        ><label class="check"
          ><input type="checkbox" bind:checked={config.reducedMotion} />{t(
            'Giảm chuyển động',
            $locale,
          )}</label
        ><label
          >{t('Ngủ sau (phút)', $locale)}<input
            type="number"
            min="1"
            max="120"
            bind:value={config.sleepMinutes}
          /></label
        ><label class="check"
          ><input type="checkbox" bind:checked={config.cursorTracking} />Nhìn
          theo con trỏ</label
        >
        <label class="check"
          ><input type="checkbox" bind:checked={config.touchReactions} />Phản
          ứng khi chạm / giữ trên đầu</label
        >
        <label class="check"
          ><input type="checkbox" bind:checked={config.clickOpensChat} />Click
          mascot để mở chat</label
        >
        <p class="muted">
          Giữ trên đầu để vuốt ve; kéo từ ngực trở lên để nhấc bổng, kéo phần
          dưới để vác lên vai. Chuột phải mở menu. Enter luôn mở chat.
        </p>
        <label class="check"
          ><input type="checkbox" disabled />{t(
            'Live2D · chưa tích hợp (Pixi v8 cần adapter riêng)',
            $locale,
          )}</label
        >
        <p class="muted">
          Skin pixel nguyên bản. Thả thư mục skin vào: {skinDirectory ||
            'xem README'}.
          <button
            onclick={async () => {
              const v = await call<{ names: string[]; directory: string }>(
                'list_skins',
              );
              skins = [...new Set(['reze', 'moon-gardener', ...v.names.filter((name) => name !== 'default')])];
              skinDirectory = v.directory;
            }}>↻ Quét skin</button
          >
        </p>{/if}
    </section>
  </div>

  <footer>
    <span role="status"
      >{t(status || 'Thay đổi được lưu tự động', $locale)}</span
    ><button
      onclick={async () => {
        try {
          config = structuredClone(defaults);
          await save();
          status = 'Đã khôi phục mặc định';
        } catch (e) {
          status = String(e);
        }
      }}>{t('Khôi phục mặc định', $locale)}</button
    >
  </footer>
</div>
