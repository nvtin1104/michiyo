<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import {
    call,
    desktop,
    type Config,
    defaults,
    type LocalPullProgress,
    type LocalRecommendedModel,
    type LocalServerStatus,
  } from '../lib/api';
  import { listen } from '@tauri-apps/api/event';
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
  const defaultGemmaModels: LocalRecommendedModel[] = [
    {
      id: 'gemma3:4b',
      label: 'Google Gemma 3 · 4B',
      sizeHint: '~3,3 GB',
      ramHint: 'RAM ≥ 8 GB',
      description:
        'Khuyên dùng cho chat, tóm tắt và dịch tiếng Việt trên máy cá nhân.',
      recommended: true,
    },
    {
      id: 'gemma3:1b',
      label: 'Google Gemma 3 · 1B',
      sizeHint: '~815 MB',
      ramHint: 'RAM ≥ 4 GB',
      description: 'Siêu nhẹ, phản hồi tức thì trên máy cấu hình thấp.',
      recommended: false,
    },
    {
      id: 'gemma3:12b',
      label: 'Google Gemma 3 · 12B',
      sizeHint: '~8,1 GB',
      ramHint: 'RAM ≥ 16 GB',
      description:
        'Chất lượng suy luận và dịch thuật cao cho máy có GPU/RAM lớn.',
      recommended: false,
    },
    {
      id: 'gemma2:9b',
      label: 'Google Gemma 2 · 9B',
      sizeHint: '~5,4 GB',
      ramHint: 'RAM ≥ 12 GB',
      description: 'Bản ổn định của Google, cân bằng tốc độ và độ chính xác.',
      recommended: false,
    },
  ];
  let localServer: LocalServerStatus | null = null;
  let pullProgress: LocalPullProgress | null = null;
  let customPullModel = '';
  let aiTab: 'local' | 'external' | 'cloudflare' =
    config.modelSource === 'cloudflare'
      ? 'cloudflare'
      : config.mode === 'offline' || config.chatProvider === 'local'
        ? 'local'
        : 'local';

  function selectAiTab(next: 'local' | 'external' | 'cloudflare') {
    aiTab = next;
    if (next === 'cloudflare') {
      config.modelSource = 'cloudflare';
      changed();
    } else if (next === 'external') {
      config.modelSource = 'external';
      if (config.chatProvider === 'local') {
        config.chatProvider = 'claude';
      }
      changed();
    } else if (next === 'local') {
      config.modelSource = 'external';
      if (config.localModel.trim()) {
        config.chatProvider = 'local';
        changed();
      }
    }
  }

  function formatBytes(bytes: number): string {
    if (!bytes || bytes <= 0) return '0 MB';
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(1).replace('.', ',')} GB`;
    const mb = bytes / (1024 * 1024);
    return `${Math.round(mb)} MB`;
  }

  async function loadLocalStatus() {
    if (!desktop) return;
    try {
      const st = await call<LocalServerStatus | null>('local_status');
      if (st && typeof st === 'object') {
        localServer = st;
        if (st.activePull) pullProgress = st.activePull;
        if (st.installedModels?.length) {
          models.local = [
            ...new Set([
              ...models.local,
              ...st.installedModels.map((m) => m.name),
            ]),
          ];
        }
      }
    } catch {
      // Ignore background probe errors so manual check remains primary
    }
  }

  async function loadModels(target: 'external' | 'local') {
    try {
      await save();
      const list = await call<string[]>('ai_models', { target });
      models[target] = Array.isArray(list) ? list : [];
      if (target === 'local') {
        await loadLocalStatus();
      }
      status = `Tìm thấy ${models[target].length} model`;
    } catch (e) {
      if (target === 'local') await loadLocalStatus();
      status = String(e);
    }
  }

  async function pullLocalModel(modelName: string) {
    const trimmed = modelName.trim();
    if (!trimmed) {
      status = 'Hãy nhập hoặc chọn tên model cần tải';
      return;
    }
    try {
      await save();
      pullProgress = {
        model: trimmed,
        status: 'pulling',
        phase: 'Đang kết nối máy chủ Ollama…',
        completedBytes: 0,
        totalBytes: 0,
        percent: 0,
        error: null,
      };
      status = `Đang tải ${trimmed}…`;
      const done = await call<LocalPullProgress>('local_pull_model', {
        model: trimmed,
      });
      if (done) pullProgress = done;
      await loadLocalStatus();
      if (!done || done.status === 'completed') {
        if (!config.localModel.trim()) {
          config.localModel = trimmed;
          await save();
        }
        status = `Đã cài xong ${trimmed}`;
        if (customPullModel.trim() === trimmed) customPullModel = '';
      } else if (done.status === 'cancelled') {
        status = `Đã huỷ tải ${trimmed}`;
      }
    } catch (e) {
      status = String(e);
      await loadLocalStatus();
    }
  }

  async function cancelLocalPull() {
    try {
      const res = await call<LocalPullProgress | null>('local_cancel_pull');
      if (res) pullProgress = res;
      status = 'Đã huỷ tải model';
    } catch (e) {
      status = String(e);
    }
  }

  async function deleteLocalModel(modelName: string) {
    try {
      await call('local_delete_model', { model: modelName });
      if (config.localModel === modelName) {
        config.localModel = '';
        await save();
      }
      models.local = models.local.filter((m) => m !== modelName);
      await loadLocalStatus();
      status = `Đã xoá model ${modelName}`;
    } catch (e) {
      status = String(e);
    }
  }

  $: recommendedList = localServer?.recommendedModels?.length
    ? localServer.recommendedModels
    : defaultGemmaModels;
  $: installedMap = new Map(
    (localServer?.installedModels ?? []).map((m) => [m.name, m]),
  );
  $: extraInstalledModels = (localServer?.installedModels ?? []).filter(
    (m) => !recommendedList.some((r) => r.id === m.name),
  );

  let show = false;
  let stored: Record<string, boolean> = {};
  let dirtyTimer: ReturnType<typeof setTimeout>;
  let skins = ['reze', 'moon-gardener'];
  let skinDirectory = '';
  onMount(() => {
    let offPull: (() => void) | undefined;
    let disposed = false;
    if (desktop) {
      call<Record<string, boolean>>('secret_status').then((v) => (stored = v));
      call<{ names: string[]; directory: string }>('list_skins').then((v) => {
        skins = [
          ...new Set([
            'reze',
            'moon-gardener',
            ...v.names.filter((name) => name !== 'default'),
          ]),
        ];
        skinDirectory = v.directory;
      });
      listen<LocalPullProgress>('local-pull-progress', (e) => {
        pullProgress = e.payload;
        if (e.payload.status === 'completed') {
          loadLocalStatus();
        }
      }).then((f) => (disposed ? f() : (offPull = f)));
    }
    return () => {
      disposed = true;
      offPull?.();
      clearTimeout(dirtyTimer);
    };
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
  <div class="settings-layout">
    <nav aria-label="Nhóm cài đặt">
      <header class="settings-header">
        <h1>{t('Cài đặt', $locale)}</h1>
      </header>
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
      {#if tab === 'Chung'}<div class="columns">
          <label
            >{t('Ngôn ngữ giao diện', $locale)}<Select
              label={t('Ngôn ngữ giao diện', $locale)}
              bind:value={config.locale}
              options={[
                { value: 'vi', label: 'Tiếng Việt' },
                { value: 'en', label: 'English' },
              ]}
              onchange={changed}
            /></label
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
        </div>
        <label
          >{t('Độ trong suốt', $locale)}<input
            type="range"
            min="0.3"
            max="1"
            step="0.05"
            bind:value={config.opacity}
          /></label
        >
        <div class="actions">
          <label class="check"
            ><input type="checkbox" bind:checked={config.autostart} />{t(
              'Khởi động cùng hệ thống',
              $locale,
            )}</label
          ><button onclick={() => call('reset_position')}
            >{t('Về vị trí mặc định', $locale)}</button
          >
        </div>
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
            <div class="compact-inline-row">
              <input
                aria-label={`${name} API key`}
                type={show ? 'text' : 'password'}
                bind:value={keys[name]}
                placeholder={t('Nhập key mới · để trống + Lưu để xóa', $locale)}
                autocomplete="off"
              />
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
      {:else if tab === 'AI'}
        {#await loadLocalStatus()}{/await}
        <div class="ai-top-bar">
          <label class="ai-mode-field"
            >Chế độ<Select
              label="Chế độ"
              bind:value={config.mode}
              options={[
                { value: 'online', label: 'Online' },
                { value: 'hybrid', label: 'Hybrid (Dự phòng cục bộ)' },
                { value: 'offline', label: 'Offline (Chỉ model trên máy)' },
              ]}
              onchange={() => {
                if (config.mode === 'offline') aiTab = 'local';
                changed();
              }}
            /></label
          >
          <div class="ai-seg-tabs" role="tablist" aria-label="Loại cài đặt AI">
            <button
              type="button"
              role="tab"
              aria-selected={aiTab === 'local'}
              class:active={aiTab === 'local'}
              onclick={() => selectAiTab('local')}
            >
              <span class="seg-dot" class:on={localServer?.online}></span>
              Google Local (Ollama)
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={aiTab === 'external'}
              class:active={aiTab === 'external'}
              onclick={() => selectAiTab('external')}
            >
              Model ngoài (Claude · OpenAI)
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={aiTab === 'cloudflare'}
              class:active={aiTab === 'cloudflare'}
              onclick={() => selectAiTab('cloudflare')}
            >
              Model Cloudflare
            </button>
          </div>
        </div>

        {#if aiTab === 'local'}
          <div class="key-card ai-pane">
            <div class="ai-pane-head">
              <h3>
                Máy chủ Ollama · Google Gemma <small
                  >{localServer?.online
                    ? `● Đang chạy${localServer.version ? ` (v${localServer.version})` : ''}`
                    : localServer
                      ? '○ Chưa kết nối (chạy ollama serve)'
                      : '○ Máy chủ cục bộ'}</small
                >
              </h3>
              <div class="actions">
                <button
                  class:primary={config.modelSource === 'external' &&
                    config.chatProvider === 'local'}
                  disabled={!config.localModel.trim()}
                  onclick={() => {
                    config.modelSource = 'external';
                    config.chatProvider = 'local';
                    changed();
                  }}
                  >{config.modelSource === 'external' &&
                  config.chatProvider === 'local'
                    ? '✓ Đang dùng Local'
                    : 'Dùng làm AI chính'}</button
                >
                <button onclick={() => loadModels('local')}
                  >Kiểm tra kết nối</button
                >
              </div>
            </div>
            <div class="columns">
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
                  placeholder="Ví dụ: gemma3:4b"
                /><datalist id="models-local"
                  >{#each models.local as id}<option value={id}
                    ></option>{/each}</datalist
                ></label
              >
            </div>
            {#if pullProgress && pullProgress.status === 'pulling'}
              <div class="local-pull-box" role="status" aria-live="polite">
                <div class="local-pull-header">
                  <strong
                    >Đang tải {pullProgress.model} ({pullProgress.percent}%)</strong
                  >
                  {#if pullProgress.totalBytes > 0}
                    <small
                      >{formatBytes(pullProgress.completedBytes)} / {formatBytes(
                        pullProgress.totalBytes,
                      )}</small
                    >
                  {/if}
                </div>
                <progress value={pullProgress.percent} max="100"></progress>
                <div class="local-pull-footer">
                  <small>{pullProgress.phase}</small>
                  <button onclick={cancelLocalPull}>Huỷ tải</button>
                </div>
              </div>
            {/if}
            <div
              class="local-model-list"
              aria-label="Danh sách model Google Gemma"
            >
              {#each recommendedList as item}
                {@const installed =
                  installedMap.get(item.id) || models.local.includes(item.id)}
                <div class="local-model-row">
                  <div class="local-model-meta">
                    <strong>
                      {item.label}
                      <code>{item.id}</code>
                      {#if item.recommended}<span class="local-badge"
                          >Khuyên dùng</span
                        >{/if}
                      <span class="local-spec"
                        >{installed && installedMap.get(item.id)?.sizeBytes
                          ? formatBytes(installedMap.get(item.id)!.sizeBytes)
                          : item.sizeHint} · {item.ramHint}</span
                      >
                    </strong>
                    <small>{item.description}</small>
                  </div>
                  <div class="actions">
                    {#if installed}
                      <button
                        disabled={config.localModel === item.id}
                        onclick={() => {
                          config.localModel = item.id;
                          changed();
                        }}
                        >{config.localModel === item.id
                          ? 'Đang chọn'
                          : 'Chọn dùng'}</button
                      >
                      <button onclick={() => deleteLocalModel(item.id)}
                        >Xoá</button
                      >
                    {:else}
                      <button
                        disabled={pullProgress?.status === 'pulling'}
                        onclick={() => pullLocalModel(item.id)}>Tải về</button
                      >
                    {/if}
                  </div>
                </div>
              {/each}
              {#each extraInstalledModels as ext}
                <div class="local-model-row">
                  <div class="local-model-meta">
                    <strong>
                      <code>{ext.name}</code>
                      <span class="local-badge">Đã cài</span>
                      <span class="local-spec"
                        >{ext.sizeBytes
                          ? formatBytes(ext.sizeBytes)
                          : 'Local'}{ext.parameterSize
                          ? ` · ${ext.parameterSize}`
                          : ''}</span
                      >
                    </strong>
                  </div>
                  <div class="actions">
                    <button
                      disabled={config.localModel === ext.name}
                      onclick={() => {
                        config.localModel = ext.name;
                        changed();
                      }}
                      >{config.localModel === ext.name
                        ? 'Đang chọn'
                        : 'Chọn dùng'}</button
                    >
                    <button onclick={() => deleteLocalModel(ext.name)}
                      >Xoá</button
                    >
                  </div>
                </div>
              {/each}
            </div>
            <div class="local-custom-pull">
              <input
                aria-label="Tên model Ollama cần tải"
                bind:value={customPullModel}
                placeholder="Tải model khác từ Ollama (vd: gemma3:27b, qwen2.5:7b)"
              />
              <button
                disabled={!customPullModel.trim() ||
                  pullProgress?.status === 'pulling'}
                onclick={() => pullLocalModel(customPullModel)}
                >Tải theo tên</button
              >
            </div>
          </div>
        {:else if aiTab === 'external'}
          <div class="key-card ai-pane">
            <div class="ai-pane-head">
              <h3>Cài đặt Model ngoài (Claude · OpenAI)</h3>
              <div
                class="ai-sub-pills"
                role="group"
                aria-label="Nhà cung cấp chat"
              >
                <button
                  type="button"
                  class:active={config.chatProvider !== 'openai_compat'}
                  onclick={() => {
                    config.modelSource = 'external';
                    config.chatProvider = 'claude';
                    changed();
                  }}>Claude (Agent SDK)</button
                >
                <button
                  type="button"
                  class:active={config.chatProvider === 'openai_compat'}
                  onclick={() => {
                    config.modelSource = 'external';
                    config.chatProvider = 'openai_compat';
                    changed();
                  }}>OpenAI-compatible</button
                >
              </div>
            </div>
            {#if config.chatProvider === 'openai_compat'}
              <div class="actions preset-row" role="group" aria-label="Preset">
                {#each presets as [name, url]}<button
                    onclick={() => {
                      config.openaiBase = url;
                      changed();
                    }}>{name}</button
                  >{/each}
              </div>
              <div class="columns">
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
                    >{#each models.external as id}<option value={id}
                      ></option>{/each}</datalist
                  ></label
                >
              </div>
              <label
                >API key ({stored.openai ? '● Đã có key' : '○ Chưa có key'})
                <div class="compact-inline-row">
                  <input
                    aria-label="openai API key"
                    type="password"
                    bind:value={keys.openai}
                    placeholder={t(
                      'Nhập key mới · để trống + Lưu để xóa',
                      $locale,
                    )}
                    autocomplete="off"
                  />
                  <button onclick={() => saveKey('openai')}
                    >{t('Lưu key', $locale)}</button
                  ><button onclick={() => loadModels('external')}
                    >Liệt kê model</button
                  >
                </div>
              </label>
            {:else}
              <div class="columns">
                <label
                  >Model Claude<Select
                    label="Model Claude"
                    bind:value={config.model}
                    options={choices(['claude-haiku-5-5', 'claude-sonnet-5-5'])}
                    onchange={changed}
                  /></label
                >
                <label
                  >Khóa Claude ({stored.claude
                    ? '● Đã có key'
                    : '○ Chưa có key'})
                  <div class="compact-inline-row">
                    <input
                      aria-label="claude API key"
                      type="password"
                      bind:value={keys.claude}
                      placeholder="Nhập key Claude mới"
                      autocomplete="off"
                    />
                    <button onclick={() => saveKey('claude')}
                      >{t('Lưu key', $locale)}</button
                    >
                  </div>
                </label>
              </div>
            {/if}
          </div>
        {:else}
          <div class="key-card ai-pane">
            <div class="ai-pane-head">
              <h3>
                Michiyo Cloud (Cloudflare Workers AI) <small
                  >{stored.cloud ? '● Đã ghép nối' : '○ Chưa ghép nối'}</small
                >
              </h3>
            </div>
            <div class="columns">
              <label
                >Địa chỉ Michiyo Cloud<input
                  aria-label="Địa chỉ Michiyo Cloud"
                  bind:value={config.cloudUrl}
                  placeholder="https://michiyo.example.workers.dev"
                /></label
              >
              <label
                >Model Cloudflare<input
                  aria-label="Model Cloudflare"
                  bind:value={config.cloudModel}
                  placeholder="@cf/meta/llama-3.1-8b-instruct"
                /></label
              >
            </div>
            <label
              >Ghép nối thiết bị
              <div class="compact-inline-row">
                {#if !stored.cloud}
                  <input
                    aria-label="Mã ghép nối"
                    bind:value={keys.cloudCode}
                    placeholder="Mã từ trang quản trị (hiệu lực 10 phút)"
                    autocomplete="off"
                  />
                  <button
                    disabled={!keys.cloudCode.trim() || !config.cloudUrl.trim()}
                    onclick={pairCloud}>Ghép nối</button
                  >
                {:else}
                  <button onclick={unpairCloud}>Ngắt kết nối thiết bị</button>
                {/if}
              </div>
            </label>
          </div>
        {/if}
      {:else if tab === 'Đồng bộ'}
        {#await loadSync()}{/await}
        <p class="muted">
          Đồng bộ ghi chú, thói quen, lịch và hoạt động giữa các thiết bị qua
          Michiyo Cloud. Chế độ Offline tắt hoàn toàn đồng bộ. Lịch sử trò
          chuyện không bao giờ được đồng bộ.
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
              Cần chế độ Online/Hybrid, địa chỉ Michiyo Cloud và thiết bị đã
              ghép nối (tab AI).
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
              bind:checked={
                config.syncScopes[name as keyof typeof config.syncScopes]
              }
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
        ><label class="check"
          ><input type="checkbox" bind:checked={config.autonomousMovement} />{t(
            'Mascot tự di chuyển',
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
          mascot để mở bong bóng chat</label
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
              skins = [
                ...new Set([
                  'reze',
                  'moon-gardener',
                  ...v.names.filter((name) => name !== 'default'),
                ]),
              ];
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

<style>
  .ai-top-bar {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 10px;
    flex-wrap: wrap;
  }
  .ai-mode-field {
    flex: 0 1 220px;
    margin: 0 !important;
  }
  .ai-seg-tabs {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: #e7ece5;
    border: 1px solid #d5ded2;
    flex: 1 1 auto;
    justify-content: stretch;
  }
  .ai-seg-tabs button {
    flex: 1 1 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 5px 10px;
    min-height: 28px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: #4b5c50;
    font-size: 11.5px;
    font-weight: 500;
    white-space: nowrap;
  }
  .ai-seg-tabs button:hover {
    background: rgba(255, 255, 255, 0.55);
  }
  .ai-seg-tabs button.active {
    background: #ffffff;
    color: #1f472f;
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(24, 45, 31, 0.1);
  }
  .seg-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #9aa79c;
    flex-shrink: 0;
  }
  .seg-dot.on {
    background: #2f9e53;
  }
  .ai-pane {
    padding: 10px 12px !important;
    margin: 0 !important;
  }
  .ai-pane-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }
  .ai-pane-head h3 {
    margin: 0 !important;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .ai-pane-head h3 small {
    float: none !important;
  }
  .ai-sub-pills {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 7px;
    background: #eef2ec;
    border: 1px solid #d9e1d6;
  }
  .ai-sub-pills button {
    border: 0;
    background: transparent;
    padding: 4px 9px;
    font-size: 11.5px;
    border-radius: 5px;
  }
  .ai-sub-pills button.active {
    background: #385e4b;
    color: #ffffff;
    font-weight: 600;
  }
  .preset-row {
    margin-bottom: 8px;
  }
  .compact-inline-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .compact-inline-row input {
    flex: 1 1 auto;
    margin-bottom: 0 !important;
  }
  .local-pull-box {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 7px 10px;
    border-radius: 7px;
    background: #eef4ef;
    border: 1px solid #cfe0d2;
    margin: 4px 0 6px;
  }
  .local-pull-header,
  .local-pull-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .local-pull-box progress {
    width: 100%;
    height: 8px;
    accent-color: #345c43;
  }
  .local-model-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-top: 4px;
  }
  .local-model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 6px;
    background: #f8faf7;
    border: 1px solid #e1e8e0;
    flex-wrap: wrap;
  }
  .local-model-meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1 1 220px;
  }
  .local-model-meta strong {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .local-model-meta code {
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 4px;
    background: #e6eee7;
    color: #264833;
  }
  .local-spec {
    font-size: 11px;
    font-weight: 400;
    color: #6b7a6e;
  }
  .local-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 999px;
    background: #345c43;
    color: #ffffff;
  }
  .local-custom-pull {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .local-custom-pull input {
    flex: 1 1 180px;
    min-width: 0;
    margin-bottom: 0 !important;
  }
</style>
