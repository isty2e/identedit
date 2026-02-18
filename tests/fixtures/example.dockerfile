FROM alpine:3.20 AS build
WORKDIR /app
ENV APP_HOME=/app
RUN echo "hello" > /app/hello.txt
COPY . .
CMD ["sh", "-c", "cat /app/hello.txt"]
