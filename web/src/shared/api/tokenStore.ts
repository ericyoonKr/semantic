export type TokenStore = {
  get(): string | null;
  set(value: string | null): void;
};

export function memoryStore(initial: string | null = null): TokenStore {
  let value = initial;
  return {
    get: () => value,
    set: (next) => {
      value = next;
    },
  };
}

export function sessionStore(key: string): TokenStore {
  const fallback = memoryStore();
  return {
    get: () => {
      try {
        return window.sessionStorage.getItem(key);
      } catch {
        return fallback.get();
      }
    },
    set: (next) => {
      fallback.set(next);
      try {
        if (next === null) {
          window.sessionStorage.removeItem(key);
        } else {
          window.sessionStorage.setItem(key, next);
        }
      } catch {
        return;
      }
    },
  };
}
