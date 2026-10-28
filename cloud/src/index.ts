import { app } from './app';
import type { Env } from './env';
import { purgeTombstones } from './sync';

export default {
  fetch: app.fetch,
  async scheduled(_event: ScheduledController, env: Env, ctx: ExecutionContext) {
    ctx.waitUntil(purgeTombstones(env));
  },
};
