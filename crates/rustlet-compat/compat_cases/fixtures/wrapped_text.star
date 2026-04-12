load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            width = 64,
            height = 32,
            color = "#000",
            child = render.Padding(
                pad = 1,
                child = render.WrappedText(
                    width = 62,
                    height = 8,
                    font = render.fonts["6x10"],
                    color = "#fff",
                    content = "wrap this line around and keep the bounds stable",
                ),
            ),
        ),
    )
