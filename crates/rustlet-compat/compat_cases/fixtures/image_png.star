load("render.star", "render")
load("encoding/base64.star", "base64")

PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEX/AAAZ4gk3AAAACklEQVR4nGNiAAAABgADNjd8qAAAAABJRU5ErkJggg=="

def main(config):
    return render.Root(
        child = render.Padding(
            pad = 4,
            child = render.Image(src = base64.decode(PNG), width = 16, height = 16),
        ),
    )
