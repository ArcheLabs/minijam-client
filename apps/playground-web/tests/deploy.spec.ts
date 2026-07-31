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
  await expect.poll(async () => page.evaluate(() =>
    localStorage.getItem("minijam.playground.services.v1")
  )).toContain('"serviceId":7');
  await page.getByRole("button", { name: "← Playground" }).click();
  await page.getByRole("button", { name: /Stage 0 test wallet/ }).click();
  await expect(page.getByRole("menu", { name: "Wallet menu" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu", { name: "Wallet menu" })).toBeHidden();
  await page.getByRole("button", { name: /Stage 0 test wallet/ }).click();
  await page.getByRole("menuitem", { name: "My Services" }).click();
  await expect(page).toHaveURL("/services");
  await expect(page.getByRole("heading", { name: "My Services" })).toBeVisible();
  await expect(page.getByText("This list contains Services deployed from this browser.")).toBeVisible();
  await page.getByRole("button", { name: "Open Service" }).click();
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
