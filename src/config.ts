export interface Config {
  host: string;
  port: number;
  publicURL: string;
  dbPath: string;
  cerebrateURL: string;
  adminToken: string;
}

export function loadConfig(): Config {
  return {
    host:         process.env.PYLON_HOST         ?? "0.0.0.0",
    port:         parseInt(process.env.PYLON_PORT ?? "15316"),
    publicURL:    process.env.PYLON_PUBLIC_URL    ?? "",
    dbPath:       process.env.PYLON_DB_PATH       ?? `${process.env.HOME ?? "/tmp"}/.openzerg/pylon.db`,
    cerebrateURL: process.env.CEREBRATE_URL       ?? "",
    adminToken:   process.env.CEREBRATE_ADMIN_TOKEN ?? "",
  };
}
