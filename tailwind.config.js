/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        goose: {
          50:  "#f0fdf4",
          100: "#dcfce7",
          200: "#bbf7d0",
          300: "#86efac",
          400: "#4ade80",
          500: "#22c55e",
          600: "#16a34a",
          700: "#15803d",
          800: "#166534",
          900: "#14532d",
        },
        surface: {
          0:   "#0d0d0d",
          1:   "#171717",
          2:   "#1f1f1f",
          3:   "#2a2a2a",
          4:   "#404040",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system", "BlinkMacSystemFont", "Segoe UI",
          "Noto Sans SC", "PingFang SC", "Microsoft YaHei",
          "sans-serif",
        ],
      },
      animation: {
        "spin-slow": "spin 2s linear infinite",
        "fade-in":   "fadeIn 0.15s ease-out",
        "slide-up":  "slideUp 0.2s ease-out",
      },
      keyframes: {
        fadeIn:  { from: { opacity: "0" }, to: { opacity: "1" } },
        slideUp: { from: { opacity: "0", transform: "translateY(8px)" }, to: { opacity: "1", transform: "translateY(0)" } },
      },
    },
  },
  plugins: [],
};
