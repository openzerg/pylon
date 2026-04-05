import { randomBytes } from "node:crypto";

export function randomId(): string {
  return randomBytes(8).toString("hex");
}

export function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}
