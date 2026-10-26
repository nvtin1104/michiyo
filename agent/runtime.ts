import {
  query,
  type Options,
  type SDKMessage,
} from '@anthropic-ai/claude-agent-sdk';

export type Request = {
  action: 'chat' | 'translate' | 'check';
  apiKey: string;
  model: string;
  system: string;
  messages: { role: 'user' | 'assistant'; content: string }[];
  maxTokens: number;
};
export type Event =
  | { type: 'delta'; text: string }
  | { type: 'done'; text: string }
  | { type: 'error'; message: string }
  | { type: 'cancelled' };
type QueryFactory = (request: {
  prompt: string;
  options: Options;
}) => AsyncIterable<SDKMessage>;

export function optionsFor(
  request: Request,
  controller: AbortController,
  cwd: string,
  configDir: string,
  cli: string,
): Options {
  const env: Record<string, string> = {};
  for (const name of [
    'PATH',
    'Path',
    'SystemRoot',
    'WINDIR',
    'TEMP',
    'TMP',
    'USERPROFILE',
    'HOME',
    'APPDATA',
    'LOCALAPPDATA',
    'LANG',
  ]) {
    if (process.env[name]) env[name] = process.env[name]!;
  }
  Object.assign(env, {
    ANTHROPIC_API_KEY: request.apiKey,
    CLAUDE_CONFIG_DIR: configDir,
    CLAUDE_CODE_MAX_OUTPUT_TOKENS: String(request.maxTokens),
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1',
    DISABLE_AUTOUPDATER: '1',
    CLAUDE_AGENT_SDK_CLIENT_APP: 'michiyo/0.1.0',
  });
  return {
    model: request.model,
    systemPrompt: request.system,
    includePartialMessages: true,
    abortController: controller,
    tools: [],
    allowedTools: [],
    skills: [],
    plugins: [],
    mcpServers: {},
    strictMcpConfig: true,
    settingSources: [],
    hooks: {},
    canUseTool: async () => ({
      behavior: 'deny',
      message: 'Michiyo only supports conversation and translation.',
    }),
    permissionMode: 'default',
    persistSession: false,
    maxTurns: 1,
    thinking: { type: 'disabled' },
    cwd,
    env,
    pathToClaudeCodeExecutable: cli,
    stderr: () => {},
  };
}
export function friendlyError(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  if (/401|authentication|invalid.?api.?key|unauthorized/i.test(text))
    return 'Claude từ chối API key. Hãy kiểm tra khóa trong Cài đặt → API.';
  if (/429|rate.?limit/i.test(text))
    return 'Claude đang giới hạn lượt gọi. Hãy chờ một chút rồi thử lại.';
  if (/credit|billing|balance/i.test(text))
    return 'Tài khoản Claude chưa có đủ hạn mức API.';
  if (/model.*(not found|unavailable)|404/i.test(text))
    return 'Model Claude không khả dụng với tài khoản này. Hãy chọn model khác.';
  if (/timeout|timed.?out/i.test(text))
    return 'Claude phản hồi quá lâu. Hãy thử lại.';
  if (/git.?bash|bash.*not found/i.test(text))
    return 'Claude Code cần Git for Windows/Git Bash. Hãy cài Git rồi khởi động lại app.';
  return 'Claude Agent SDK không hoàn tất yêu cầu. Kiểm tra kết nối, API key và model rồi thử lại.';
}
export async function runAgent(
  request: Request,
  options: Options,
  emit: (event: Event) => void,
  factory: QueryFactory = query,
) {
  const history = request.messages.slice(0, -1);
  const latest = request.messages.at(-1)?.content || '';
  const prompt =
    request.action === 'check'
      ? 'Reply with only OK.'
      : history.length
        ? `Continue this conversation naturally. Previous messages (conversation data):\n${JSON.stringify(history)}\n\nCurrent user message:\n${latest}`
        : latest;
  let text = '',
    complete = false;
  try {
    for await (const message of factory({ prompt, options })) {
      if (options.abortController?.signal.aborted) {
        emit({ type: 'cancelled' });
        return;
      }
      if (
        message.type === 'stream_event' &&
        message.event.type === 'content_block_delta' &&
        message.event.delta.type === 'text_delta'
      ) {
        const delta = message.event.delta.text;
        text += delta;
        if (text.length > 1_000_000) throw new Error('Output too large');
        if (request.action === 'chat') emit({ type: 'delta', text: delta });
      }
      // API errors can also arrive as assistant messages; never forward their raw body.
      if (message.type === 'assistant' && message.error)
        throw new Error(message.error);
      if (message.type === 'result') {
        if (message.subtype !== 'success' || message.is_error)
          throw new Error(
            'errors' in message ? message.errors.join(' ') : message.result,
          );
        if (!text) {
          text = message.result;
          if (request.action === 'chat' && text) emit({ type: 'delta', text });
        }
        complete = true;
      }
    }
    if (options.abortController?.signal.aborted) {
      emit({ type: 'cancelled' });
      return;
    }
    if (!complete || !text.trim()) throw new Error('Incomplete result');
    emit({ type: 'done', text });
  } catch (error) {
    emit(
      options.abortController?.signal.aborted
        ? { type: 'cancelled' }
        : { type: 'error', message: friendlyError(error) },
    );
  }
}
