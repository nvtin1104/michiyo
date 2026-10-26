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
  let keys: Record<string, string> = { claude: '', google: '', libre: '' };
  let show = false;
  let stored: Record<string, boolean> = {};
  let dirtyTimer: ReturnType<typeof setTimeout>;
  let skins = ['default'];
  let skinDirectory = '';
  onMount(() => {
    if (desktop) {
      call<Record<string, boolean>>('secret_status').then((v) => (stored = v));
      call<{ names: string[]; directory: string }>('list_skins').then((v) => {
        skins = v.names;
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
    <span>Michiyo</span>
  </header>
  <div class="settings-layout">
    <nav aria-label="Nhóm cài đặt">
      {#each ['Chung', 'API', 'Chat', 'Dịch', 'Phím tắt', 'Giao diện'] as name}<button
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
              label: value === 'default' ? 'Moon gardener' : value,
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
              skins = v.names;
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
