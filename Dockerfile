FROM node:alpine as tailwindcss
WORKDIR /tmp
COPY package*.json postcss.config.js tailwind.config.js ./
ADD components ./components
ADD app ./app
ADD styles ./styles
RUN npm install
RUN npx tailwindcss -i styles/global.css -o style.css --minify

FROM python:3.11-slim
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/
WORKDIR /app
COPY ./pyproject.toml ./uv.lock ./
RUN uv sync --frozen --no-dev --no-editable
COPY app ./app
ADD components ./components
COPY public ./public
COPY --from=tailwindcss /tmp/style.css ./public/style.css
EXPOSE 80
ENV PATH="/app/.venv/bin:$PATH"
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "80", "--proxy-headers", "--forwarded-allow-ips", "'*'"]
