load("render.star", "render")
load("encoding/base64.star", "base64")

GIF = "R0lGODlhBQAEAPAAAAAAAAAAACH5BAF7AAAAIf8LTkVUU0NBUEUyLjADAQAAACwAAAAABQAEAAACBgRiaLmLBQAh+QQBewAAACwAAAAABQAEAAACBYRzpqhXACH5BAF7AAAALAAAAAAFAAQAAAIGDG6Qp8wFACH5BAF7AAAALAAAAAAFAAQAAAIGRIBnyMoFADs="

def main(config):
    return render.Root(
        child = render.Padding(
            pad = 2,
            child = render.Image(src = base64.decode(GIF)),
        ),
    )
