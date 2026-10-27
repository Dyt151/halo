import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        /* 强调色：琥珀色阶（品牌色，仅用于主操作 / 选中态 / 聚焦光环） */
        accent: {
          DEFAULT: "#E8A33D",
          50: "#FDF6EC",
          100: "#F9E8CD",
          200: "#F2D0A0",
          300: "#EDB874",
          400: "#F0B35A",
          500: "#E8A33D",
          600: "#D68F2B",
          700: "#B3741F",
          800: "#8A5816",
          900: "#5F3D10",
          on: "#1C1C1E",
          tint: "rgba(232,163,61,0.14)",
          "tint-text": "#F0B35A",
          disabled: "rgba(232,163,61,0.32)",
        },
        /* 背景层级（对照 NSColor dark） */
        win: "#1E1E1E",
        group: "#2C2C2E",
        raised: "#3A3A3C",
        /* 预览正文：走 CSS 变量，深色 #E8E8ED / 浅色 #1C1C1E（清单 25）。
         * 写死字面量会让浅色模式下的卡片正文白底白字（已踩过一次）。 */
        preview: "var(--text-preview)",
        /* 语义色（soft = 用于 12% 提示条/浅色文字的同系亮色） */
        danger: { DEFAULT: "#FF6969", fill: "#FF453A", soft: "#FF8A80" },
        success: { DEFAULT: "#32D74B", soft: "#5FE08C" },
        warning: "#FFD60A",
        favorite: "#FF6969",
        /* 类型徽章（规范 §2.5：14% 彩底 + 同色系亮字） */
        badge: {
          text: "rgba(10,132,255,0.14)",
          "text-fg": "#64A8FF",
          image: "rgba(48,209,88,0.14)",
          "image-fg": "#5FE08C",
          files: "rgba(232,163,61,0.14)",
          "files-fg": "#F0B35A",
          html: "rgba(191,90,242,0.14)",
          "html-fg": "#C98BF5",
          unknown: "rgba(255,255,255,0.10)",
          "unknown-fg": "rgba(255,255,255,0.50)",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "SF Pro Text",
          "SF Pro Display",
          "PingFang SC",
          "Helvetica Neue",
          "sans-serif",
        ],
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
      /* Apple 缓动曲线 */
      transitionTimingFunction: {
        apple: "cubic-bezier(0.32, 0.72, 0, 1)",
        "out-soft": "cubic-bezier(0.4, 0, 0.2, 1)",
        "in-soft": "cubic-bezier(0.4, 0, 1, 1)",
      },
      transitionDuration: {
        120: "120ms",
      },
      boxShadow: {
        menu: "0 0 0 0.5px rgba(0,0,0,0.55), 0 8px 32px rgba(0,0,0,0.50)",
        popover: "0 0 0 0.5px rgba(0,0,0,0.5), 0 12px 40px rgba(0,0,0,0.55)",
        modal: "0 0 0 0.5px rgba(0,0,0,0.5), 0 24px 80px rgba(0,0,0,0.60)",
        "select-ring": "0 0 0 1px rgba(232,163,61,0.45)",
      },
      keyframes: {
        "panel-in": {
          from: { opacity: "0", transform: "translateY(10px) scale(0.985)" },
          to: { opacity: "1", transform: "none" },
        },
        "fade-in": { from: { opacity: "0" }, to: { opacity: "1" } },
        "pop-in": {
          from: { opacity: "0", transform: "scale(0.96) translateY(-4px)" },
          to: { opacity: "1", transform: "none" },
        },
        /* 面板收起（评审 U9）：与 Panel 的 CLOSE_ANIM_MS = 140 对齐。 */
        "panel-out": {
          from: { opacity: "1", transform: "none" },
          to: { opacity: "0", transform: "scale(0.985) translateY(6px)" },
        },
        /* 对话框从窗口顶部展开（评审 U3）：仿 NSAlert sheet 的下落手感。 */
        "sheet-in": {
          from: { opacity: "0", transform: "translateY(-12px) scale(0.98)" },
          to: { opacity: "1", transform: "none" },
        },
        /* Toast 自底部升起（评审 E1）。 */
        "toast-in": {
          from: { opacity: "0", transform: "translateY(12px) scale(0.97)" },
          to: { opacity: "1", transform: "none" },
        },
      },
      animation: {
        "panel-in": "panel-in 200ms cubic-bezier(0.32, 0.72, 0, 1)",
        "panel-out": "panel-out 140ms cubic-bezier(0.4, 0, 1, 1) forwards",
        "sheet-in": "sheet-in 180ms cubic-bezier(0.32, 0.72, 0, 1)",
        "toast-in": "toast-in 180ms cubic-bezier(0.32, 0.72, 0, 1)",
        "fade-in": "fade-in 150ms cubic-bezier(0.4, 0, 0.2, 1)",
        "pop-in": "pop-in 150ms cubic-bezier(0.32, 0.72, 0, 1)",
      },
    },
  },
  plugins: [],
} satisfies Config;
