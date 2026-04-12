load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            width = 128,
            height = 64,
            color = "#112233",
            child = render.Padding(
                pad = 4,
                child = render.Text(
                    content = "2x default",
                    color = "#ddeeff",
                ),
            ),
        ),
    )
