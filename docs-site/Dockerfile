FROM oven/bun:1 AS builder
WORKDIR /app
COPY package.json bun.lockb* ./
RUN bun install --frozen-lockfile || bun install
COPY . .
RUN bun run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html/oxidizedgraph
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
EXPOSE 80
ENV PORT=80
