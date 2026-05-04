FROM python:3.13-slim
RUN pip install "databases[aiosqlite]==0.8.0"
RUN pip install "shortuuid==1.0.11"
WORKDIR /app
USER root
