struct PushConstants
{
    float4x4 model;
    float4x4 light_view_proj;
};
[[vk::push_constant]]
PushConstants pc;

struct CameraUniforms
{
    float4x4 view;
    float4x4 proj;
    float4x4 viewProj;
    float time;
    uint light_count;
    uint _pad0;
    uint _pad1;
};
[[vk::binding(0, 0)]]
ConstantBuffer<CameraUniforms> camera;

struct GpuLight
{
    float4x4 view_proj; // light's own view*proj, meaningful only if shadow_layer >= 0
    float4 direction;   // xyz = normalized travel direction, w unused
    int shadow_layer;   // index into the shadow map array, or -1 if unshadowed
    uint check_outside; // should surface outside shadow map be lit
    uint _pad[2];
};
[[vk::binding(1, 0)]]
StructuredBuffer<GpuLight> lights;

[[vk::binding(0, 1)]]
[[vk::combinedImageSampler]]
Texture2DArray shadowMaps;
[[vk::binding(0, 1)]]
[[vk::combinedImageSampler]]
SamplerComparisonState shadowSampler;

struct VSInput
{
    float4 pos;
    float3 normal;
    float4 color;
};

struct VSOutput
{
    float4 position : SV_Position;
    float3 worldNormal : TEXCOORD0;
    float3 worldPos : TEXCOORD1;
};

[shader("vertex")]
VSOutput vertMain(VSInput input)
{
    VSOutput output;
    float4 worldPos = mul(pc.model, float4(input.pos.xyz, 1.0));
    output.position = mul(camera.viewProj, worldPos);
    output.worldNormal = mul((float3x3)pc.model, input.normal);
    output.worldPos = worldPos.xyz;
    return output;
}

[shader("fragment")]
float4 fragMain(VSOutput input) : SV_Target
{
    float3 N = normalize(input.worldNormal);
    float ambient = 0.2;
    float3 diffuse = float3(0, 0, 0);
    for (uint i = 0; i < camera.light_count; i++)
    {
        GpuLight L = lights[i];
        float ndotl = max(dot(N, -normalize(L.direction.xyz)), 0.0);
        // clamp dot product to 0 - same effect
        if (ndotl <= 0.0)
            continue;

        float lit = 1.0;
        if (L.shadow_layer >= 0)
        {
            float4 lsp = mul(L.view_proj, float4(input.worldPos, 1.0));
            float3 proj = lsp.xyz / lsp.w;
            float2 shadowUV = proj.xy * 0.5 + 0.5;
            bool out_of_range = lsp.w <= 0.0 || any(shadowUV < 0.0) || any(shadowUV > 1.0) || proj.z < 0.0 || proj.z > 1.0;
            if (out_of_range)
            {
                lit = L.check_outside;
            }
            else
            {
                lit = shadowMaps.SampleCmp(shadowSampler, float3(shadowUV, (float)L.shadow_layer), proj.z);
            }
        }
        diffuse += lit * ndotl;
    }
    float3 shading = float3(ambient) + diffuse;

    return float4(shading, 1.0);
}

[shader("vertex")]
float4 shadow(float4 pos) : SV_Position
{
    return mul(pc.light_view_proj, mul(pc.model, float4(pos.xyz, 1.0)));
}
