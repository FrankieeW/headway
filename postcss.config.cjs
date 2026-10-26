// Tailwind v4 把 PostCSS 插件拆到了独立包 @tailwindcss/postcss，
// tailwindcss 自身的入口不再能作为 PostCSS 插件使用。
module.exports = {
  plugins: {
    "@tailwindcss/postcss": {},
  },
};
