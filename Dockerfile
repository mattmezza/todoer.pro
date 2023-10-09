FROM node:alpine as tailwindcss
WORKDIR /tmp
COPY package*.json postcss.config.js tailwind.config.js ./
ADD htm ./htm
ADD styles ./styles
RUN npm install
RUN npx tailwindcss -i styles/global.css -o style.css --minify

FROM python:3.11-slim as builder
WORKDIR /tmp
RUN pip install poetry
COPY ./pyproject.toml ./poetry.lock* /tmp/
RUN poetry export -f requirements.txt --output requirements.txt --without-hashes

FROM python:3.11-slim
COPY --from=builder /tmp/requirements.txt /tmp/requirements.txt
RUN pip install --no-cache-dir --upgrade -r /tmp/requirements.txt
WORKDIR /app
COPY app ./app
COPY htm ./htm
COPY public ./public
COPY --from=tailwindcss /tmp/style.css ./public/style.css
EXPOSE 80
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "80", "--proxy-headers", "--forwarded-allow-ips", "'*'"]
