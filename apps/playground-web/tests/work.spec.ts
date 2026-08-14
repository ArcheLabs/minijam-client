import { expect, test } from "@playwright/test";
import { initialState, mockPlaygroundApi } from "./mock-api";

test("runs Work, survives reload, and observes Counter state", async ({ page }) => {
  const state = initialState();
  await mockPlaygroundApi(page, state);
  await page.goto("/services/7");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await page.getByRole("button", { name: "Run Work" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page).toHaveURL(/operations\/work-op/);
  await page.reload();
  await expect(page.getByText("Completed")).toBeVisible({ timeout: 5000 });
  await page.getByRole("button", { name: "View finalized Service state" }).click();
  await page.getByRole("button", { name: "Read finalized value" }).click();
  await expect(page.getByText("Counter: 1")).toBeVisible();
});

test("owner mismatch keeps Work available but disables Upgrade", async ({ page }) => {
  const state = initialState();
  state.controller = `0x${"99".repeat(32)}`;
  await mockPlaygroundApi(page, state);
  await page.goto("/services/7");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await expect(page.getByText(/Upgrade remains Controller-only/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Run Work" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Upgrade Service" })).toBeDisabled();
});

test("shows an explicit result when finalized storage has no value", async ({ page }) => {
  const state = initialState();
  state.storageMissing = true;
  await mockPlaygroundApi(page, state);
  await page.goto("/services/7");
  await page.getByRole("button", { name: "Read finalized value" }).click();

  await expect(page.getByText("No finalized value exists for this key.")).toBeVisible();
  await expect(page.getByText(`Finalized block: 0x${"77".repeat(32)}`)).toBeVisible();
});
