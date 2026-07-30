import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const base = env.VITE_APP_BASE_PATH || '/issue-management';

  return {
    base: base.endsWith('/') ? base : `${base}/`,
    plugins: [react()],
    server: {
      port: 5173,
    },
  };
});

