import { expect, test } from "vitest";
import source from "./NewProject.tsx?raw";

test("new project includes Web Server selection and submits it", () => {
  expect(source).toContain("Web Server");
  expect(source).toContain("web_server: webServer || null");
  expect(source).toContain("!webInstalled");
});
