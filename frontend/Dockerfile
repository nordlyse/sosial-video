FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm install
COPY . .
ARG VITE_CONTACT_API=http://localhost:8081
ARG VITE_WEBRTC_API=http://localhost:8082
ENV VITE_CONTACT_API=$VITE_CONTACT_API
ENV VITE_WEBRTC_API=$VITE_WEBRTC_API
RUN npm run build

FROM node:22-alpine
WORKDIR /app
RUN npm install -g serve
COPY --from=build /app/dist ./dist
EXPOSE 3000
CMD ["serve", "-s", "dist", "-l", "3000"]
