<script lang="ts">
  import { onMount } from 'svelte';

  let status = 'Đang xác thực phiên đăng nhập...';
  let error = '';

  onMount(async () => {
    try {
      const hash = window.location.hash.slice(1);
      const params = new URLSearchParams(hash);
      const code = params.get('code');
      if (!code) {
        error = 'Thiếu mã đăng nhập. Hãy mở trang quản trị từ ứng dụng Michiyo Desktop (Cài đặt → Cloud riêng → Mở trang quản trị).';
        status = '';
        return;
      }

      const res = await fetch('/api/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ code })
      });

      if (res.ok) {
        status = 'Đăng nhập thành công! Đang chuyển hướng...';
        window.location.replace('/');
      } else {
        const body = await res.json().catch(() => ({}));
        error = body.error === 'invalid_code' 
          ? 'Mã đăng nhập không hợp lệ hoặc đã hết hạn (chỉ có hiệu lực trong 60 giây). Hãy lấy mã mới từ Desktop.'
          : 'Không thể tạo phiên đăng nhập. Vui lòng thử lại từ Desktop.';
        status = '';
      }
    } catch (e) {
      error = 'Lỗi kết nối khi đăng nhập: ' + String(e);
      status = '';
    }
  });
</script>

<div class="login-container">
  <div class="card">
    <h2>Michiyo Cloud Admin</h2>
    {#if status}
      <p class="status">{status}</p>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
  </div>
</div>

<style>
  .login-container {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 80vh;
    font-family: system-ui, sans-serif;
  }
  .card {
    background: #18181b;
    color: #f4f4f5;
    padding: 2rem;
    border-radius: 0.75rem;
    box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.3);
    max-width: 420px;
    width: 100%;
    text-align: center;
  }
  h2 {
    margin-top: 0;
    margin-bottom: 1rem;
    font-size: 1.25rem;
  }
  .status {
    color: #a1a1aa;
    line-height: 1.5;
  }
  .error {
    color: #f87171;
    line-height: 1.5;
    background: rgba(239, 68, 68, 0.1);
    padding: 0.75rem;
    border-radius: 0.375rem;
    border: 1px solid rgba(239, 68, 68, 0.2);
  }
</style>
