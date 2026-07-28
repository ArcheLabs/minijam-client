import { expect, test } from "@playwright/test";
import { initialState, mockPlaygroundApi } from "./mock-api";

test("connects, builds, signs Create, and reaches the Service", async ({ page }) => {
  const state = initialState();
  await mockPlaygroundApi(page, state);
  await page.goto("/");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await page.getByRole("button", { name: "Build service" }).click();
  await page.getByRole("button", { name: "Deploy service" }).click();
  await expect(page.getByRole("dialog")).toContainText("Create Service");
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page).toHaveURL(/operations\/create-op/);
  await expect(page.getByText("Completed")).toBeVisible({ timeout: 5000 });
  await page.getByRole("button", { name: "Open Service 7" }).click();
  await expect(page.getByRole("heading", { name: "Service 7" })).toBeVisible();
});

test("shows signed action replay explicitly", async ({ page }) => {
  const state = initialState();
  state.replayAction = "used";
  await mockPlaygroundApi(page, state);
  await page.goto("/");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await page.getByRole("button", { name: "Build service" }).click();
  await page.getByRole("button", { name: "Deploy service" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.locator(".error-panel")).toContainText("already used");
});
