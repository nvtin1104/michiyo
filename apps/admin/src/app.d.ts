declare global {
  namespace App {
    interface Platform {
      env: { MICHIYO_API?: { fetch(request: Request): Promise<Response> } };
    }
  }
}
export {};
