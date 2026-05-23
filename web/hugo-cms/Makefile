.PHONY: serve build clean

serve:
	hugo server --buildDrafts --navigateToChanged

build:
	hugo --gc --minify

clean:
	rm -rf public/ resources/
