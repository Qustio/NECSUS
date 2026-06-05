struct VSOutput
{
    float4 position : SV_Position;
    float2 ndc : TEXCOORD0;
};

struct VSInput
{
    float2 position;
};

[shader("vertex")]
VSOutput vertMain(VSInput input)
{
    VSOutput output;
    output.position = float4(input.position, 1.0, 1.0);
    output.ndc = input.position * 0.5 + 0.5;
    return output;
}

[shader("fragment")]
float4 fragMain(VSOutput input) : SV_Target
{
    return float4(0.0, input.ndc.x / 4, 0.0, input.ndc.x);
}
