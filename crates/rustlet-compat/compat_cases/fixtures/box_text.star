load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            width = 64,
            height = 32,
            color = "#101820",
            child = render.Padding(
                pad = 2,
                child = render.Text(
                    content = "compat",
                    height = 10,
                    font = render.fonts["6x13"],
                    color = "#f2aa4c",
                ),
            ),
        ),
    )
