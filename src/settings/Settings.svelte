<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { call, desktop, type Config, defaults } from '../lib/api';
  import { onMount } from 'svelte';
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

<header>
  <div>
    <small>MAKE YOURSELF AT HOME</small>
    <h1>{t('Cài đặt', $locale)} <span>✦</span></h1>
  </div>
  <span class="tag">MICHIYO / 01</span>
</header>
<div class="settings-layout">
  <nav>
    {#each ['Chung', 'API', 'Chat', 'Dịch', 'Phím tắt', 'Giao diện'] as name, i}<button
        class:active={tab === name}
        onclick={() => (tab = name)}
        ><span>0{i + 1}</span>{t(name, $locale)}<b>↗</b></button
      >{/each}
    <div class="nav-note">
      A little companion.<br />A little everyday magic.
    </div>
  </nav>
  <section class="settings-content" onchange={changed}>
    <h2>{t(tab, $locale)}</h2>
    <p class="muted">{t('Điều chỉnh không gian nhỏ của bạn.', $locale)}</p>
    {#if tab === 'Chung'}<label
        >{t('Ngôn ngữ giao diện', $locale)}<select bind:value={config.locale}
          ><option value="vi">Tiếng Việt</option><option value="en"
            >English</option
          ></select
        ></label
      ><label class="check"
        ><input type="checkbox" bind:checked={config.autostart} />{t(
          'Khởi động cùng hệ thống',
          $locale,
        )}</label
      ><label
        >{t('Kích thước mascot', $locale)}<select bind:value={config.scale}
          >{#each [1, 2, 3] as scale}<option value={scale}>{scale}x</option
            >{/each}</select
        ></label
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
        >{t('Vị trí mặc định', $locale)}<select bind:value={config.anchor}
          ><option value="bottom-right">↘ Bottom right</option><option
            value="bottom-left">↙ Bottom left</option
          ><option value="top-right">↗ Top right</option><option
            value="top-left">↖ Top left</option
          ></select
        ></label
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
        >Model<select bind:value={config.model}
          ><option>claude-haiku-5-5</option><option>claude-sonnet-5-5</option
          ></select
        ></label
      ><label
        >System prompt<textarea rows="6" bind:value={config.systemPrompt}
        ></textarea></label
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
          >{t('Nguồn', $locale)}<select bind:value={config.source}
            >{#each ['auto', 'vi', 'en', 'ja', 'zh', 'ko', 'fr', 'de'] as lang}<option
                >{lang}</option
              >{/each}</select
          ></label
        ><label
          >{t('Đích', $locale)}<select bind:value={config.target}
            >{#each ['vi', 'en', 'ja', 'zh', 'ko', 'fr', 'de'] as lang}<option
                >{lang}</option
              >{/each}</select
          ></label
        ><label
          >{t('Đích thay thế', $locale)}<select bind:value={config.alternate}
            ><option>en</option><option>vi</option><option>ja</option></select
          ></label
        >
      </div>
      <label>{t('Lĩnh vực', $locale)}<input bind:value={config.domain} /></label
      ><label
        >{t('Giọng văn', $locale)}<select bind:value={config.tone}
          ><option>Tự nhiên</option><option>Trang trọng</option><option
            >Thân mật</option
          ></select
        ></label
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
        >{t('Skin', $locale)}<select bind:value={config.skin}
          >{#each skins as name}<option value={name}
              >{name === 'default' ? 'Michiyo · Moon gardener' : name}</option
            >{/each}</select
        ></label
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
  </section>
</div>
