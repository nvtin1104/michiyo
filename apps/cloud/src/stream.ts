/**
 * Converts a Workers AI event stream (`data: {"response":"…"}`) into OpenAI-style chunks
 * (`choices[0].delta.content`), so clients such as the desktop app need one SSE parser.
 */
export function toOpenAiStream(
  source: ReadableStream<Uint8Array>,
  model: string,
  onFinish: (outputChars: number) => void,
): ReadableStream<Uint8Array> {
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  let pending = '';
  let chars = 0;
  let finished = false;
  const chunk = (delta: object, finish: string | null = null) =>
    encoder.encode(`data: ${JSON.stringify({ object: 'chat.completion.chunk', model, choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`);
  const finish = (controller: TransformStreamDefaultController<Uint8Array>) => {
    if (finished) return;
    finished = true;
    controller.enqueue(chunk({}, 'stop'));
    controller.enqueue(encoder.encode('data: [DONE]\n\n'));
    onFinish(chars);
  };
  const handle = (line: string, controller: TransformStreamDefaultController<Uint8Array>) => {
    const payload = line.trim().startsWith('data:') ? line.trim().slice(5).trim() : null;
    if (payload === null) return;
    if (payload === '[DONE]') return finish(controller);
    try {
      const text = JSON.parse(payload).response;
      if (typeof text === 'string' && text) {
        chars += text.length;
        controller.enqueue(chunk({ content: text }));
      }
    } catch {
      // Ignore keep-alives and malformed lines.
    }
  };
  return source.pipeThrough(
    new TransformStream<Uint8Array, Uint8Array>({
      transform(bytes, controller) {
        pending += decoder.decode(bytes, { stream: true });
        const lines = pending.split('\n');
        pending = lines.pop() ?? '';
        for (const line of lines) handle(line, controller);
      },
      flush(controller) {
        if (pending) handle(pending, controller);
        finish(controller);
      },
    }),
  );
}
