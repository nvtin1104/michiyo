<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';

  import { call, desktop } from '../lib/api';
  export let compact = false;
  let text = '';
  let result: {
    text: string;
    engine: string;
    source: string;
    target: string;
  } | null = null;
  let busy = false;
  let error = '';
  let seq = 0;
  async function translate(engine?: string) {
    const id = ++seq;
    busy = true;
    error = '';
    try {
      const answer = await call<typeof result>('translate', {
        text,
        engine: engine || null,
      });
      if (id === seq) result = answer;
    } catch (e) {
      if (id === seq) error = String(e);
    } finally {
      if (id === seq) busy = false;
    }
  }
  onMount(() => {
    let off: undefined | (() => void);
    let offError: undefined | (() => void);
    let offNavigate: undefined | (() => void);
    const takeDraft = async () => {
      if (compact) return;
      try {
        const draft = await call<{
          text: string;
          result: typeof result;
        } | null>('take_translation_draft');
        if (draft) {
          text = draft.text;
          result = draft.result;
        }
      } catch (e) {
        error = String(e);
      }
    };
    const take = async () => {
      const pending = await call<{ text?: string; error?: string } | null>(
        'take_selection',
      );
      if (pending?.text) {
        text = pending.text;
        translate();
      } else if (pending?.error) {
        error = pending.error;
      }
    };
    if (desktop) {
      if (!compact) {
        listen<string>('shell-navigate', (event) => {
          if (event.payload === 'translate') takeDraft();
        }).then((f) => (offNavigate = f));
        takeDraft();
      }
      if (compact) {
        listen('selection', take).then((f) => {
          off = f;
          take();
        });
        listen<string>('selection-error', (e) => (error = e.payload)).then(
          (f) => (offError = f),
        );
      }
    }
    return () => {
      off?.();
      offError?.();
      offNavigate?.();
    };
  });
</script>

<header>
  <div>
    <small>WORDS, WITH CONTEXT</small>
    <h1>{t('Dịch ngữ cảnh', $locale)} <span>↔</span></h1>
  </div>
  {#if compact}<button
      disabled={busy}
      onclick={() =>
        call('open_translation_in_main', { text, result }).catch(
          (e) => (error = String(e)),
        )}>Mở trong app ↗</button
    >{/if}
</header>
<p class="muted">
  {t('Chọn văn bản ở ứng dụng bất kỳ, nhấn Ctrl + Alt + T.', $locale)}
</p>
<textarea
  aria-label={t('Văn bản cần dịch', $locale)}
  bind:value={text}
  placeholder={t('Dán văn bản cần dịch…', $locale)}></textarea>
<div class="toolbar">
  <span class="tag">{t('Tự nhận diện ngôn ngữ', $locale)}</span><button
    class="primary"
    disabled={busy || !text.trim()}
    onclick={() => translate()}
    >{busy
      ? $locale === 'en'
        ? 'Translating…'
        : 'Đang dịch…'
      : $locale === 'en'
        ? 'Translate ↗'
        : 'Dịch ↗'}</button
  >
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if result}<article>
    <small>{result.source} → {result.target} · {result.engine}</small>
    <p class="translation">{result.text}</p>
    <div class="actions">
      <button onclick={() => call('copy_text', { text: result!.text })}
        >{t('Sao chép', $locale)}</button
      ><select
        aria-label="Dịch lại bằng engine khác"
        onchange={(e) => {
          if (e.currentTarget.value) translate(e.currentTarget.value);
          e.currentTarget.value = '';
        }}
        ><option value="">{t('Engine khác', $locale)}</option
        >{#each ['mymemory', 'libre', 'google', 'claude'] as engine}<option
            value={engine}>{engine}</option
          >{/each}</select
      ><button
        onclick={async () => {
          await call('ask_translation', {
            text: `Giải thích bản dịch sau:\n${text}\n\n${result!.text}`,
          });
        }}>{t('Hỏi thêm ↗', $locale)}</button
      >
    </div>
  </article>{/if}
