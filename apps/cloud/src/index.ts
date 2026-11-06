import { app } from './app';
import type { Env } from './env';
import { processDelivery, purgeExpiredDeliveries, recoverPendingDeliveries } from './telegram';

export default {
  fetch: app.fetch,
  async queue(batch: MessageBatch<{ updateId: number; payload: string; receivedAt: number }>, env: Env) {
    for (const message of batch.messages) {
      try { await processDelivery(env, message.body); message.ack(); }
      catch {
        console.error('telegram_queue_processing_failed');
        if (message.attempts >= 5) {
          try {
            const result = await env.DB.prepare("UPDATE telegram_webhook_inbox SET status = 'failed', updated_at = ? WHERE update_id = ? AND status = 'received'")
              .bind(Date.now(), message.body.updateId).run();
            if (result.meta.changes) message.ack(); else message.retry();
          } catch { message.retry(); }
        } else message.retry();
      }
    }
  },
  async scheduled(_event: ScheduledController, env: Env) {
    await purgeExpiredDeliveries(env);
    await recoverPendingDeliveries(env);
  },
};
