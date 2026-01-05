# tylax Web - Docker Deployment
# PRE-REQUISITE: Run `npm run build` locally before building this image!
# Uses pre-built dist directory

FROM nginx:alpine

# 复制 nginx 配置
COPY nginx.conf /etc/nginx/conf.d/default.conf

# 复制构建产物
COPY dist/ /usr/share/nginx/html/

# 添加 WASM MIME 类型（在 types 块的末尾添加）
RUN sed -i '/^}/i\    application/wasm                                 wasm;' /etc/nginx/mime.types

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]

