import { screen, fireEvent, waitFor } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import { afterEach, it, expect, vi } from "vitest";
import GuideAvatarCropper from "./GuideAvatarCropper";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it("lets the user choose crop position and zoom before importing", async () => {
  const drawImage = vi.fn();
  const revokeObjectURL = vi.fn();
  vi.stubGlobal("Image", class {
    naturalWidth = 800; naturalHeight = 400;
    onload: (() => void) | null = null;
    set src(_value: string) { queueMicrotask(() => this.onload?.()); }
  });
  vi.stubGlobal("URL", { createObjectURL: () => "blob:avatar-test", revokeObjectURL });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ clearRect: vi.fn(), drawImage } as any);
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,cropped");
  const onApply = vi.fn().mockResolvedValue(undefined);
  const { unmount } = render(<GuideAvatarCropper file={new File(["fixture"], "portrait.png")}
    onApply={onApply} onCancel={vi.fn()} />);
  await waitFor(() => expect(drawImage).toHaveBeenCalled());
  expect(onApply).not.toHaveBeenCalled();
  fireEvent.change(screen.getByRole("slider", { name: "缩放" }), { target: { value: "2" } });
  fireEvent.change(screen.getByRole("slider", { name: "左右位置" }), { target: { value: "100" } });
  fireEvent.change(screen.getByRole("slider", { name: "上下位置" }), { target: { value: "0" } });
  expect(drawImage).toHaveBeenLastCalledWith(expect.anything(), -768, -0, 1024, 512);
  fireEvent.click(screen.getByRole("button", { name: "使用此头像" }));
  await waitFor(() => expect(onApply).toHaveBeenCalledWith("data:image/png;base64,cropped"));
  unmount();
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:avatar-test");
});
