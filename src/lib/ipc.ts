import { invoke } from '@tauri-apps/api/core';

export const api = {
  system: {
    getAppVersion: (): Promise<string> => invoke<string>('get_app_version'),
  },
};
