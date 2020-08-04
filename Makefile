.PHONY: docker_server docker_client

docker_server:
	docker run --rm -it --cap-add NET_ADMIN -p 3000:3000 -v $(shell pwd):/app -w /app simple_vpn bash

docker_client:
	docker run --rm -it --cap-add NET_ADMIN -v $(shell pwd):/app -w /app simple_vpn bash
