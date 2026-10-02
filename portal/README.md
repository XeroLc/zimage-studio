# Z-Image Studio 项目门户

产品落地页：纯静态站点（Svelte 5 + SvelteKit 3 静态导出 + GSAP/Motion），
视觉语言延续应用本体（ComfyUI charcoal / azure 色板）。

## 开发与构建

```bash
npm install
npm run dev        # 本地预览 http://localhost:5173
npm run build      # 输出到 build/（全静态，任意子路径可部署）
npm run preview
```

## 部署到 GitHub Pages

仓库自带的 workflow（`.github/workflows/deploy-portal.yml`）会在推送到 `main` 后
自动构建并发布。只需在 GitHub 仓库设置里做一次性配置：

**Settings → Pages → Build and deployment → Source 选 “GitHub Actions”**

之后每次改动 `portal/` 或 workflow 并推送，站点自动更新。

> 站点用相对路径构建，部署到 `https://<用户名>.github.io/<仓库名>/` 无需改任何配置。

## 需要改的地方

- `src/lib/site.ts`：仓库地址（默认 `https://github.com/XeroLc/zimage-studio`）。
  仓库名不同时只改这一处，导航与下载按钮的链接都会跟着变。
- `static/shots/`：界面截图；替换图片时保持文件名不变即可。
