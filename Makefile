.PHONY: docker_image docker_build docker_run docker_image_arm32v7 docker_build_arm32v7

docker_image:
	docker build -f docker/default.dockerfile docker -t simple_tunnel

docker_build:
	docker run --rm -it -v $(shell pwd):/app -w /app simple_tunnel cargo build --release

docker_run:
	docker run --rm -it --cap-add NET_ADMIN -v $(shell pwd):/app -w /app simple_tunnel bash

docker_image_arm32v7:
	docker build -f docker/arm32v7.dockerfile docker -t arm32v7/simple_tunnel

docker_build_arm32v7:
	docker run --rm -it -v $(shell pwd):/app -w /app arm32v7/simple_tunnel cargo build --release --target armv7-unknown-linux-gnueabihf
