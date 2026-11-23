# Builds the frontend's static assets into /srv/dist. The compose `frontend`
# service copies them to the host's ./frontend/dist, which the backend serves.
FROM node:22-slim AS build
WORKDIR /app
COPY package.json yarn.lock ./
RUN yarn install --frozen-lockfile
COPY . .
RUN yarn build

FROM node:22-slim
COPY --from=build /app/dist /srv/dist
# The compose command copies /srv/dist to the mounted volume.
