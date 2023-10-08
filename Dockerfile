FROM node:alpine as tailwindcss
WORKDIR /tmp
COPY package*.json postcss.config.js tailwind.config.js /tmp/
ADD styles /tmp/styles
RUN npm install
RUN npx tailwindcss -i styles/global.css -o style.css

FROM python:3.11-slim as builder
WORKDIR /tmp
RUN pip install poetry
COPY ./pyproject.toml ./poetry.lock* /tmp/
RUN poetry export -f requirements.txt --output requirements.txt --without-hashes

FROM python:3.11-slim
WORKDIR /app
COPY --from=builder /tmp/requirements.txt /tmp/requirements.txt
RUN pip install --no-cache-dir --upgrade -r /tmp/requirements.txt
ADD . /app
COPY --from=tailwindcss /tmp/style.css /tmp/public/style.css
EXPOSE 80
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "80", "--proxy-headers", "--forwarded-allow-ips", "'*'"]
