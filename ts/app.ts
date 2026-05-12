class Spinner {
  private el: HTMLElement;

  constructor(container: HTMLElement) {
    this.el = document.createElement('div');
    this.el.className = 'spinner';
    this.el.style.cssText = 'display:inline-block;width:20px;height:20px;border:2px solid #30363d;border-top-color:#58a6ff;border-radius:50%;animation:spin-ani 0.6s linear infinite;';
    container.appendChild(this.el);
  }

  remove(): void {
    this.el.remove();
  }
}

type ToastType = 'success' | 'error' | 'info';

class Toast {
  static show(message: string, type: ToastType = 'info', duration = 4000): void {
    const el = document.createElement('div');
    el.className = `toast ${type}`;
    el.textContent = message;
    el.style.cssText = 'position:fixed;bottom:20px;right:20px;padding:12px 20px;border-radius:8px;color:white;font-size:0.9rem;z-index:1000;animation:slideIn 0.3s ease;';
    document.body.appendChild(el);
    setTimeout(() => el.remove(), duration);
  }
}

class DarkModeToggle {
  private static KEY = 'deko-theme';

  static init(): void {
    const saved = localStorage.getItem(this.KEY);
    if (saved === 'light') this.apply('light');
    const btn = document.getElementById('theme-toggle');
    btn?.addEventListener('click', () => {
      const next = document.documentElement.getAttribute('data-theme') === 'light' ? 'dark' : 'light';
      this.apply(next);
      localStorage.setItem(this.KEY, next);
    });
  }

  private static apply(theme: string): void {
    document.documentElement.setAttribute('data-theme', theme);
  }
}

document.addEventListener('DOMContentLoaded', () => {
  DarkModeToggle.init();
});

const styleSheet = document.createElement('style');
styleSheet.textContent = `
@keyframes spin-ani { to { transform: rotate(360deg); } }
@keyframes slideIn { from { transform: translateY(20px); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
`;
document.head.appendChild(styleSheet);
