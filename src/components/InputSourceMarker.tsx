import { useId } from "react";
import type { ReactNode } from "react";

/**
 * 无图标输入源的字牌（正版 `labelChip`）：18×15、圆角 4.5 的实心 `.primary`
 * 底，10pt semibold 白字以 `.blendMode(.destinationOut)` 挖空。
 *
 * CSS 的 `mix-blend-mode` 没有 `destinationOut`（那是 Canvas/SwiftUI 的值），
 * 所以用等价的 SVG mask：遮罩里白=留、黑=挖，字牌本体是 `currentColor`。
 *
 * 面板的输入法二级页和切换 HUD 共用同一枚字牌，两处必须一模一样。
 */
export function InputSourceMarker({
  marker,
  fallback,
}: {
  marker: string | null | undefined;
  /** 读不到 1~2 字标记时的替代图标（正版退回符号）。 */
  fallback: ReactNode;
}) {
  const maskId = useId();
  if (!marker) return <>{fallback}</>;
  return (
    <svg
      width="18"
      height="15"
      viewBox="0 0 18 15"
      className="shrink-0 text-white"
      aria-hidden
    >
      <defs>
        <mask id={maskId}>
          <rect width="18" height="15" fill="#fff" />
          <text
            x="9"
            y="7.5"
            fill="#000"
            fontSize="10"
            fontWeight="600"
            textAnchor="middle"
            dominantBaseline="central"
          >
            {marker}
          </text>
        </mask>
      </defs>
      <rect width="18" height="15" rx="4.5" fill="currentColor" mask={`url(#${maskId})`} />
    </svg>
  );
}
