.PHONY: docker_server docker_client docker_image_arm32v7 docker_build_arm32v7

docker_server:
	docker run --rm -it --cap-add NET_ADMIN -p 3000:3000 -v $(shell pwd):/app -w /app simple_vpn bash

docker_client:
	docker run --rm -it --cap-add NET_ADMIN -v $(shell pwd):/app -w /app simple_vpn bash

docker_image_arm32v7:
	docker build -f docker/arm32v7.dockerfile docker -t arm32v7/simple_tunnel

docker_build_arm32v7:
	docker run --rm -it -v $(shell pwd):/app -w /app arm32v7/simple_tunnel cargo build --release --target armv7-unknown-linux-gnueabihf
