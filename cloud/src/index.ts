import { app } from './app';
import type { Env } from './env';
import { storeDelivery } from './instagram';

export default {
  fetch: app.fetch,
  async queue(batch: MessageBatch<{ payload: string; receivedAt: number }>, env: Env) {
    for (const message of batch.messages) {
      try { await storeDelivery(env, message.body); message.ack(); }
      catch { message.retry(); }
    }
  },
};
