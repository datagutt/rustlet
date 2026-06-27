load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            width = 64,
            height = 32,
            color = "#101820",
            child = render.Text(
                content = "Ay",
                font = "tb-8",
                offset = 3,
                color = "#f2aa4c",
            ),
        ),
    )
