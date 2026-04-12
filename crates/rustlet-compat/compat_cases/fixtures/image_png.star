load("render.star", "render")
load("encoding/base64.star", "base64")

PNG = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAEAQMAAACTPww9AAAABlBMVEUAAP////973JksAAAAE0lEQVR4nGNgoDpgYGAAAOQAAZ0M0w0AAAAASUVORK5CYII="

def main(config):
    return render.Root(
        child = render.Padding(
            pad = 4,
            child = render.Image(src = base64.decode(PNG), width = 16, height = 16),
        ),
    )
