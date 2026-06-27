load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            width = 64,
            height = 32,
            color = "#101820",
            child = render.WrappedText(
                content = "jghpqy AB",
                font = "tom-thumb",
                width = 20,
                linespacing = 2,
                color = "#f2aa4c",
            ),
        ),
    )
