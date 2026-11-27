// Not implemented yet (#1112); the tests in isolatedHome.test.ts describe it.

/** The variables `isolateHome` sets. */
export interface IsolatedHome {
  HOME: string;
  XDG_CONFIG_HOME: string;
  XDG_CACHE_HOME: string;
  XDG_DATA_HOME: string;
  XDG_STATE_HOME: string;
  GIT_CONFIG_GLOBAL: string;
  GIT_CONFIG_NOSYSTEM: string;
  GNUPGHOME: string;
}

export function isolateHome(root: string): IsolatedHome {
  void root;
  return {} as IsolatedHome;
}
